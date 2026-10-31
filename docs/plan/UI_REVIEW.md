# DNA Calc UI review

Reviewed 2026-10-30 for **dc-971p — DNA Calc UI: Excel-parity grid and skin polish**.
Review/tools task: dc-971p.1. Implementation tasks: dc-971p.2–dc-971p.15.

DNA Calc is already useful for opening a workbook, inspecting calculated cells, making basic
edits, and looking at dependencies. It is not yet a dependable replacement for Excel's grid.
The biggest obstacle is the editing contract: the cell editor and formula bar disagree about
commit and navigation, and ordinary range/navigation/clipboard operations are incomplete.
Build that contract before adding more controls. The other skins have useful inspection
functions, but their names sometimes promise more authoring than they currently offer.

## Evidence and limits

- Local source: master at `485c4b6831d9223870c18f591f11882c70bd2ef0`, after
  `git pull --no-rebase`. No product or harness source changed for this review.
- Published DNA Calc: [dnacalc.com](https://dnacalc.com), release
  `20261030T120637Z-a029649a9c2b`; OneCalc:
  [dnaonecalc.dnacalc.com](https://dnaonecalc.dnacalc.com), release
  `20261030T121556Z-a029649a9c2b`. Their `/build.json` files identify source
  `a029649a9c2b3fdebd5bae246e8df03bc5266105`. Direct browser loads and manifest fetches were
  used; a search engine's older landing-page snapshot is not evidence of the current site.
  `git diff a029649a9c2b..485c4b6 -- apps tools/xtask/src/app.rs` is empty. Calculation
  libraries have changed since publication, so published numerical results are not attributed
  to the newer local build.
- Read `AGENTS.md`, `WORK_SETS.md`, `BULK_PORT_PLAN.md`, the architecture plan (especially
  §§8.1–8.7 and 9), all six skin implementations, shell, skin registry, controller, worker
  transport, shared formula UI, OneCalc session/surface, Strand tokens and `STRAND.md`.
  There is no file named `STRAND_DESIGN_LANGUAGE.md` in this checkout; `STRAND.md` is its
  retained copy and explicitly makes the token crate authoritative.
- Browser inspection uses real rendered pages at **1440×1000, 1024×768 and 390×844 CSS pixels**,
  device scale 1. Chrome supplies screenshots and exploratory keyboard/pointer observations;
  the existing app smokes exercise Chrome and Firefox. The desktop-window smoke at 500 px does
  not establish the usability of a 390 px viewport or touch input.
- Reused fixtures: `calc-host/tests/fixtures/dc_gf0/cut-base.xlsx` (three sheets, names and
  dependencies), `dc_09v/formats.xlsx`, `dc_6yw/conditional-formats.xlsx`,
  `dc_6yw/data-validation.xlsx`, and `calc-doc/tests/fixtures/dc_5sr/features.xlsx`.
  These paths are under `crates/`. The last file actually contains frozen panes, H1:I1 merged,
  styled cells, wrapped text, a comment on A1 and a hyperlink at K1. Its package XML was read
  alongside the rendered page; preservation in a package is not credited as UI support.
  Small interactive examples were typed into a new workbook, not installed as new fixtures.
- Excel is not installed here. This is a browser/source review, not a new live Excel parity
  capture, native desktop review, screen-reader certification or numerical agreement claim.
  The keyboard reference is Microsoft's [Excel keyboard documentation](https://support.microsoft.com/en-us/accessibility/excel/keyboard-shortcuts-in-excel),
  used for behaviour, not visual assets. Future edge cases need pinned Windows Excel observations
  where documented behaviour is insufficient.

## Browser tools and reproduction

Everything installed for Step 0 is under `/home/koderbot`; no sudo, system package installation,
Trust Center change or repository code fix was needed.

| Tool | Installed version / location |
|---|---|
| Chrome for Testing | 156.0.8078.80, `~/.local/share/dnacalc-browsers/chrome/linux-156.0.8078.80/chrome-linux64/chrome` |
| ChromeDriver | 156.0.8078.80, sibling `chromedriver/linux-156.0.8078.80/chromedriver-linux64/chromedriver` |
| Firefox | 159.0, sibling `firefox/linux-stable_159.0/firefox/firefox` |
| GeckoDriver | 0.37.1, `~/.local/share/dnacalc-browsers/geckodriver` |
| Screenshot runner | Puppeteer Core under `~/codex-runs/ui-tools`; `~/codex-runs/ui-review.mjs` |

The three Puppeteer browser downloads used `npx --yes @puppeteer/browsers install
chrome@stable`, `chromedriver@stable` and `firefox@stable`, each with
`--path ~/.local/share/dnacalc-browsers`. GeckoDriver came from Mozilla's release archive.
Firefox needed GTK/Pango/Wayland/X libraries absent on this host. Ubuntu `.deb` packages were
downloaded with `apt-get download` and unpacked with `dpkg-deb -x` into the browser directory's
`sysroot/`. Firefox 159 also requires a newer NSS than the host's 3.114; Debian's
`libnss3_3.132-1_amd64.deb` was unpacked there. These files only affect processes using the
environment below. Archives and the GeckoDriver release metadata remain in the home directory.

`tools/xtask/src/app.rs:662` finds drivers through `CHROMEDRIVER` and `GECKODRIVER`, then PATH.
ChromeDriver finds `google-chrome` on PATH. `~/.local/bin/google-chrome` and `firefox` are links
to the downloaded browsers. `~/.local/bin/chromedriver` links to the matching driver.
`~/.local/bin/geckodriver` is a small shell wrapper that sets the local library path and invokes
the real driver with `--binary` pointing to the downloaded Firefox. Thus the existing harness
needs no new browser-binary option.

```bash
cd ~/projects/DnaCalc/DnaCalc
source ~/codex-runs/ui-browser-env.sh
# Sets PATH, CHROMEDRIVER, GECKODRIVER and LD_LIBRARY_PATH to the home installs.
cargo xtask app smoke calc       # routine core, all six skins
cargo xtask app smoke onecalc    # routine formula/session core
# Reuse an existing bundle during a UI iteration; a filter searches all cases.
cargo xtask app smoke calc --bundle target/dnacalc-bundle --group alignment
cargo xtask app smoke onecalc --bundle target/onecalc-bundle --case "replace its code"
cargo xtask app smoke calc --suite full
cargo xtask app smoke onecalc --suite full
cargo xtask app smoke calc --suite full --list
```

The core has 32 cases across both apps. `--group` and `--case` can repeat; groups match their
exact names, and case names match a substring without regard to case. Each group starts a fresh
page with cleared storage/downloads and a restored viewport. A filtered later case runs its
earlier group prerequisites with every assertion intact; failures there fail the run too.
Output reports selected cases, prerequisite counts and wall time per group, including page
setup. Browser totals exclude bundle builds. A filter selecting no case fails with exit 2.
`--list` requires neither browser nor bundle. Full suites also run in `cargo xtask sweep`
(the `app-smoke` step); site smoke retains every ordinary case. Worker-fault/latency cases
need a local bundle and run in sweep, not against the site URL.

Run cargo commands sequentially. Bundles are `target/dnacalc-bundle` and
`target/onecalc-bundle`. For repeat screenshots, serve them in separate terminals:

```bash
python3 -m http.server 8743 --bind 127.0.0.1 --directory target/dnacalc-bundle
python3 -m http.server 8742 --bind 127.0.0.1 --directory target/onecalc-bundle
```

```bash
node ~/codex-runs/ui-review.mjs local
node ~/codex-runs/ui-review.mjs published
```

The runner loads actual apps, uploads the existing fixtures through their file input, types
examples, switches skins, resizes the viewport and captures screenshots. Its JSON observations
and logs live in `~/codex-runs/`. It is review tooling outside the repository, not a second app
test harness. Future regression checks belong in the existing `cargo xtask app smoke` cases.

## Screenshot catalogue

All screenshots are in **`~/codex-runs/ui-shots/`**, intentionally outside git. Names use
`{local|published}-{scene}-{width}x{height}.png`. Each scene below has all three sizes.
They show observed current behaviour, not proposed designs.

| Scene | What to inspect |
|---|---|
| `calc-empty` | Blank grid, large always-visible iteration controls, no useful first action beyond New/Open. |
| `calc-grid` | `cut-base.xlsx`, Data!B1 selected, formula `=A1*10`, value 10; amber selection and restrained grid. Footer and sheet tabs are cramped at desktop height. |
| `calc-canvas` | The same B1: A1 on the left, five readers on the right, inspector beside it. Literal input/value duplicated; long addresses split in the inspector. |
| `calc-atlas` | 22 formula cells calculated in this fixture; sheet census and diagnostics. At 390 px the wide table needs lateral access to its state columns. |
| `calc-ledger` | 22 formula rows with formula/value/state/read-count columns. At 390 px the first columns dominate; controls wrap without a clear reading order. |
| `calc-tree` | Defined names and sheet/row/cell outline; B1 selected. Useful source/value comparison, but repeated addresses and row scaffolding consume space. |
| `calc-notebook` | Data's 13 authored cells rendered as blocks. Each literal repeats its input, value and provenance sentence; only part of the sheet fits on screen. |
| `calc-features` | Imported styles/merge/comment/panes fixture. Number format and CF display work, but bold/wrap/merge/comment/link affordances are absent. |
| `calc-formats` | Thousands separators, percentages, dates, negative sections and custom text; stored row/column sizing and hiding. |
| `calc-conditional` | Imported rule colors, bars, original icons and selected number format; this is display, not a rule editor. |
| `calc-validation` | A1's list and input prompt; options are mouse targets rather than a complete keyboard list interaction. |
| `calc-spill-blocked` | `=SEQUENCE(3,2)` at C1 blocked by 99 at D2: `#SPILL!`, without an explanation pointing to D2. |
| `calc-refused` | `WEBSERVICE` refusal, distinct from a calculated worksheet error. Exact reason is in the selected cell's footer. |
| `calc-stale` | Manual mode, A1 changed to 9 while B1 still displays 10 with stale styling/text; the earlier action status can still say “Recalculated”. |
| `calc-dark`, `calc-high-contrast` | Existing token themes applied to the mixed-state grid; theme switching does not solve layout or semantics. |
| `onecalc-empty`, `onecalc-sum` | Formula workbench; A1=1.5, A2=2, blank A3 gives SUM=3.5. Result is printed twice. At 390 px the catalogue comes before scratch inputs. |
| `onecalc-array`, `onecalc-error`, `onecalc-refused` | 3×2 array, `#DIV/0!` with meaning, and unknown-function diagnostic with no value, respectively. |

## What a person can do today

All Calc skins share New/Open/Save, workbook Undo/Redo, Automatic/Manual, Calculate, iteration
controls, sheet tabs, name box, formula bar and themes. File bytes stay in the browser.
The shell and controller keep one workbook across skin changes. Save generates a download;
it does not prove the file reached durable storage. New/Open need an unsaved-work decision.
Changing skin cancels the shell draft; skin-local state is rebuilt when the view remounts.

| Surface | Useful capability today | Limit and proposed cut |
|---|---|---|
| **Grid** | Select a cell/rectangle/axis; arrows and basic Home/Tab/Enter; type/F2/double-click edit; Delete; internal copy/cut/paste; Ctrl+D/R; whole-axis insertion/deletion; scroll; zoom buttons; imported number formats, sizes, hidden axes, CF, DV prompts/list and some charts. Formula bar has highlighting, completion and signature help. | Editing is inconsistent, system clipboard absent, formatting/layout tools incomplete. Keep active address, source and result state; put iteration limits and technical timings behind Calculation/Details. Name “Insert” should say what will be inserted. |
| **Canvas** | Inspect the active cell's real dependency neighbourhood; click a chip to select it; extend backward/forward depth using buttons or `[`/`]`; Esc resets depth; F9 calculates. Inspector gives source, value, provenance, format and reads/readers. | This is a dependency inspector, not the durable movable-card canvas in architecture §8.6. Depth is capped at six, twelve chips per column, and ranges are terminal summaries. Chips are click-only divs. Remove duplicate literal input/value and repeated inspector prose; add explicit omissions, keyboard source navigation and pan/zoom/reset before authoring gestures. |
| **Atlas** | Read workbook/sheet census, actual result provenance counts, calculation mode, refusal groups, last timings and wasm memory. No skin-specific edit command. | Useful diagnostic truth, weak navigation: sheet rows and refusal groups do not take you to a fix; only the first 30 reasons are listed. Lead with “what needs attention”, then clickable sheets. Hide revision/intent/memory details. “100% calculated” is not “100% matched Excel”. |
| **Ledger** | Audit formula cells across workbook or current sheet; filter errors, provenance, text or readers of active cell; sort by address/value; group by provenance; page 100 rows; click a row to select its source. | The formula table excludes literal inputs by design. No keyboard row navigation or inline reason/recovery. All read/reader/format columns compete with the primary formula/value/state. Hide these optional columns; keep filters, counts and a reason drawer; preserve filter/page on return. |
| **Tree** | Fold workbook/names/sheets/rows; Up/Down move focus, Left/Right fold, Enter selects a cell; inspect source/value/provenance; focused cell gets dependent count. Formula bar edits the selected cell. | It organizes physical worksheet rows, not user-defined block hierarchy. Initially 50 rows/sheet, at most 30 cells/row; dynamic spill children are omitted. Keep a useful outline but do not imply hierarchy changes Excel scope. Collapse row scaffolding, offer search and labelled columns, preserve folds; reserve dependent counts for inspection. |
| **Notebook** | Read sheet cells in row order, forty blocks/page; edit an existing input and Enter to commit through the same host; see result/refusal/provenance. Spilled values can appear without editable source. | No blank-sheet creation flow, prose/markdown, explicit block order, insertion, linking, run policy or durable layout editor. Inputs are plain unlabelled fields, with no shared assist or Esc handler. Stop repeating every literal three ways. A selected block must update active source; add cell-backed creation and trustworthy editing before richer prose. |
| **OneCalc** | Evaluate one formula against a 12×8 scratch grid and typed inputs on named fixture sheets; choose dynamic/legacy form and formula address; inspect scalar/error/array, exact decimal where supplied, diagnostics and reference reads; search catalogue and insert functions; shared assist/help and themes. | Scratch cells are typed inputs, not a sheet of dependent formulas. En-US/1900 settings are fixed; there is no fixture save/restore/share, trace workbench, format/locale UI or UDF UI. Cut library names from the opening sentence, duplicate result text and default expanded read logs. Put editable inputs ahead of the function catalogue on phones. |

Architectural boundary: shared editing/highlighting/help/value/diagnostic presentation belongs
in `apps/dna-formula-ui`, with narrow host-supplied traits such as `SheetNames`. OneCalc and
that library must remain free of calc-engine, calc-doc/model and calc-host. Workbook selection,
spills, subscriptions, worker messages and skins stay in `apps/dna-calc`. Do not make OneCalc
a small workbook engine or copy its controller into Calc.

## Grid parity inventory

**Done** means the stated narrow behaviour was exercised; **partial** means a useful subset
exists; **missing** means there is no working application route for the requested surface.
These are UI assessments, not claims about every underlying library. **Critical** interrupts
ordinary work or risks losing/misreading work; **high** matters for common workbooks; **medium**
is useful but can follow dependable entry. Task suffixes refer to epic dc-971p.

| Excel grid surface | State | Current evidence and missing behaviour | Importance / slice |
|---|---|---|---|
| Single-cell selection | Done | Click/name box sets active source; formula bar and footer follow; other skins share that selection. | Critical; preserve in .2/.4. |
| Ranges and whole axes | Partial | Shift+arrows/click, mouse drag, headers and sheet corner exist; no disjoint ranges, keyboard axis selection or range/name-box input. Drag does not extend by scrolling beyond the viewport. | High; .4. |
| Basic navigation | Partial | Arrows, Tab/Shift+Tab, Enter, Home/Ctrl+Home work. Ctrl+Down from populated A1:A3 stays A1. Shift+Enter in select mode stays A2; Ctrl+End and page keys have no app mapping. | Critical; .4. |
| Navigation edges | Partial | Coordinates clamp; name box validates XFD1048576. Hidden-axis skipping, data-region jumps, full-address-space scrolling and far-viewport geometry are not established by the small smoke fixtures. | High; .4/.7. |
| Enter/Tab within selected ranges | Missing | Enter from B1:B3 moves to B2 and removes the range. No bounded range walk, reverse walk or Ctrl+Enter selected-range entry. | High; .2/.4. |
| Type-to-replace cell entry | Done | Smoke types values/formula through grid focus and checks dependents. Overlay receives subsequent input and accepted entry moves. | Critical; preserve in .2. |
| F2, double-click and Esc | Partial | F2 seeds existing authored text; append then Esc restores it. Cell editor has no completion/highlighting, reference-point mode, F4 handling or IME composition guard. | Critical; .2. |
| Formula bar editing | Partial | Shared highlighted textarea, completion and signatures; Enter commits but stays in the same cell. Tab leaves focus on grid with draft still open; navigation can then do nothing until cancellation. Bar passes no diagnostic spans and does not mount shared function help. | Critical; .2. |
| Pointing to references | Missing | Clicking B4 while bar holds `=A1+` tries to commit invalid syntax and refuses; it does not insert B4. Selecting cells during formula authoring needs a dedicated edit mode. | Critical; .2. |
| Fill | Partial | Ctrl+D/R relocate formulas over a rectangle through host copy. No fill handle, series/options or cancel preview. | High; .5. |
| Internal copy/cut/paste | Partial | Smoke checks relocation, moves and undo. Clipboard stores source coordinates, not a snapshot of copied content. No copy outline or Paste Special UI. | High; .5. |
| System copy/paste | Missing | Ctrl+C/V/X are intercepted for the controller's private clipboard. External text/HTML/Excel paste is not read by that route; native paste into the formula text is not range paste. | Critical; .5. |
| Workbook and text undo | Partial | Toolbar/Ctrl+Z/Ctrl+Y restore edits and cross-sheet selection; fill/move are host operations. Browser-local text undo and workbook undo are not presented consistently across editors; buttons remain enabled at empty history. | Critical; .2/.3/.5. |
| Row/column insert/delete | Partial | Whole-header selection plus Insert/Delete or Ctrl++/− works and transforms references/formats. A normal cell selection is refused with a helpful instruction; no shift-cells dialog or context command. | High; .7/.9. |
| Resize / auto-fit | Partial | Imported width/height affects geometry. No drag edge, size entry or auto-fit control. | High; .7. |
| Hide / unhide | Partial | Stored hidden axes have zero size. No UI to hide/unhide or navigate around hidden cells reliably. | High; .4/.7. |
| Freeze / split panes | Missing | `features.xlsx` has frozenSplit at B2; renderer has one viewport and no pane projection. Ordinary scrolling does not implement those stored panes. | High; .7. |
| Sheet tabs | Partial | Existing sheets switch; undo can return to the edited sheet. Switching resets selection to A1. No create/rename/reorder/delete/hide or Ctrl+Page sheet navigation. | High; .7. |
| Number formats as displayed | Partial | Existing fixture checks separators, percentages, dates, negative sections, exponent and literal suffix; CF can choose format. Unsupported codes silently fall back to General in visible pixels (`data-format` records it); unshowable values use fixed `#####`. No width-sensitive overflow or format control. | High; .6. |
| Alignment as displayed | Missing | Basic number-right/text-left/logical-centred defaults exist. Stored alignment/wrap/rotation/indent are not projected to cells; `features.xlsx` B5's multiline text clips. | High; .6. |
| Fonts / fill / borders as displayed | Partial | CF supplies font/fill changes and format text can choose color. Ordinary style font/bold/fill/border is absent from `CellShown`; B2's stored bold is not rendered. Font substitutions have no notice. | High; .6. |
| Merged cells | Missing | H1:I1 metadata exists in `features.xlsx`; gridlines still divide the area and hit testing remains cell-by-cell. No merge-aware selection/edit/render or merge command. | High; .6. |
| Conditional formats as shown | Partial | Colors, scales, data bars, original icon SVGs and chosen formats are exercised; updates follow edits. No applies-to/precedence inspector or rule manager; bar rendering is not full Excel signed-axis/border fidelity. | High; .8. |
| Data validation as shown | Partial | List button/Alt+Down, input prompt, chosen value and Stop rejection work. List items have click handlers, no keyboard option model. Warning/Information keeps the entry immediately, without a user choice. No rule authoring UI. | High; .8. |
| Comments / notes / hyperlinks | Missing | Source fixture has A1 comment and K1 hyperlink. No comment marker/read UI or actionable hyperlink is rendered. Saving opaque parts does not make them usable. | Medium; .8. |
| Spill ranges | Partial | Smoke verifies spill expansion/shrink and removal of old children. No visible extent/anchor relationship, read-only child explanation or spill-aware range selection. | High; .3/.4. |
| `#SPILL!` | Partial | D2=99 blocks C1's 3×2 SEQUENCE and C1 displays a calculated `#SPILL!`. No blocker address, jump or recovery action; a calculation status alone explains nothing. | High; .3/.13. |
| Find / Replace / Go To | Partial | Single A1 name-box jump exists. No app Find/Replace, named/range Go To or special-cell search; browser Find is not workbook Find. | High; .4/.9. |
| Zoom | Partial | +/− geometry controls and percent readout exist (50–400% clamp); no direct percentage, reset or pointer-centred zoom. It resets on skin remount and displayed text does not scale with geometry. | Medium; .7. |
| Status aggregates | Missing | Footer gives last action plus active-cell provenance, not selection Sum/Count/Average/Min/Max. Long messages clip or wrap awkwardly. | High; .3/.9. |
| Context menus | Missing | No app contextmenu handler or Shift+F10 command menu. Only toolbar and remembered shortcuts expose edits. | High; .9. |

## Critical findings and cuts

1. **One edit must mean one thing.** `app.rs:136` handles bar Enter/Esc, while
   `grid/mod.rs:578` handles cell Enter/Tab/Esc. The bar does not handle Tab or advance on Enter.
   Reproduction: edit A2 to 20 in the bar, Enter (still A2); edit to 25, Tab (grid focused,
   authored input still 20, draft 25). This is a practical loss-of-control problem. Unify the
   edit session, focus and commit policy before adding formatting dialogs. Keep worksheet
   mutations in the controller and shared assist in the shared library.
2. **Provenance is useful, but currently too easy to miss.** Refused cells have hatching and
   “refused”; stale values have amber tint and the footer explains them. A calculated `#NAME?`
   or `#SPILL!` is a different state from refusal. Keep those distinctions. Show a short state
   chip near the active value, with a keyboard-openable explanation: what happened, which
   address/dependency blocks it, what can be done. Show a package owner only when actual typed
   data supplies one; do not infer it from an error string. See `cells.rs:302` and the shell
   footer at `app.rs:422`.
3. **Last-action text is not calculation state.** After switching to Manual and changing A1,
   B1 correctly shows a stale 10 while the action line can retain “Recalculated in … ms”.
   Worker startup/failure/pending state is mostly a root attribute (`port.rs:17`), not a useful
   visible readout. Separate current calculation state from historical timing and distinguish
   calculated coverage from Excel agreement. The app has no per-cell Excel oracle result from
   which to make a match badge.
4. **Imported appearance can change interpretation.** The plain grid conceals merges, emphasis,
   wrapped text and review context. This is more serious than a theme mismatch. `CellShown`
   (`cells.rs:61`) has number/conditional formatting but no ordinary style/merge/view payload;
   `grid/mod.rs:617` can only draw what it receives. Project styles/panes/merges through the
   existing host/worker route; do not patch CSS for particular fixtures.
5. **The shell spends scarce space on specialist controls.** Iterate/count/change are always
   visible, including a wide unlabelled-in-place 0.001 field. Hide limits until iteration is
   enabled or Calculation settings opened. Keep document/dirty state, file actions, useful
   history, calculation state and skin choice. Move byte counts, revisions, intent counts,
   allocator memory and open/edit milliseconds to Details. Footer status/tabs need a protected
   height; the desktop screenshots visibly squeeze them.
6. **Accessibility needs application semantics, not just Tab stops.** DOM inspection finds only
   tablist/tab roles in Calc: no grid/cell or tree/treeitem semantics and no live status/error
   announcement. Grid Tab cycles cells without an explicit keyboard exit. Canvas div chips and
   Ledger table rows are pointer-only selections. Notebook fields and Ledger selectors lack
   accessible names. Open is a label over a hidden input. Add a bounded semantic projection,
   focus restoration and a documented escape route; preserve discoverable browser alternatives.
   Strand token contrast tests are valuable but do not verify every composed/CF color pair.
7. **Narrow layouts need priorities.** At 390 px the mast takes several rows, wide audit tables
   need deliberate horizontal navigation, and OneCalc puts up to sixty catalogue entries before
   the input grid. Resizing the refused-cell scene from desktop leaves F1 mostly beyond the
   right edge and its explanation below the visible footer. Put result, problem and input ahead
   of discovery/diagnostics and reveal the active cell after viewport changes. The mouse-event
   grid handlers do not establish touch selection/drag parity; validate real touch/pointer and
   IME paths later instead of crediting a responsive CSS rule as working interaction.

Strand's petrol chrome, paper, amber focus and quiet teal structure are a good base. Keep them.
Use one spacing/radius system and meaningful state text. A custom icon shape or subtle border
must remain distinguishable without color. Never copy Microsoft's icons, artwork or trade dress.
Do not rebuild a ribbon merely to look like Excel: make the same work possible through a small,
searchable command surface, keyboard routes and honest unavailable reasons.

## Implementation order and done-when slices

**First: dc-971p.2, the edit session.** A person must be able to enter, amend, point, commit,
cancel and continue from either editor without changing the wrong cell or losing a draft.
Add real key/pointer cases to the existing smoke, checking authored source and undo effects as
well as displayed text. Include completion-open Esc/Enter/Tab, composition, rejected input and
cross-skin focus. This has more immediate value than another visual control.

The table is the recommended order. Tracker blocking links represent actual shared work
(entry before navigation; navigation before range clipboard; shared state before skin polish),
not a requirement to finish every grid feature before touching a small skin improvement.
Each bead contains detailed done-when criteria and validation on the existing route.

| Order / bead | Slice and closing result |
|---|---|
| 1 — **dc-971p.2** | Cell/bar edit contract: F2/Esc/Enter/Tab, point mode/F4, completion/IME and draft/undo/focus agree in both browsers. |
| 2 — **dc-971p.3** | Shared value-state explanation and shell: visible pending/refused/cache/stale/error distinctions, useful recovery, accessible semantics/announcements, safe dirty-work transitions and readable chrome at all three sizes. |
| 3 — **dc-971p.4** | Selection/navigation: region and viewport keys, range walks, hidden/full-grid edges, multi-area/axis selection and name/range navigation work through the real app. |
| 4 — **dc-971p.5** | External/internal clipboard, Paste Special and fill: real TSV/HTML, reference/style fidelity, conflict/cancel, series and one undo group. |
| 5 — **dc-971p.6** | Imported visual fidelity plus Format Cells: ordinary styles, wrapping, merges and width-dependent display; preview/undo and save/reopen retain intent. |
| 6 — **dc-971p.7** | Axis layout, panes, zoom and sheets: resize/auto-fit/hide/freeze, structural commands, tab lifecycle and retained view/selection. |
| 7 — **dc-971p.8** | Rule/review surfaces: explain/edit CF, keyboard DV and warning decisions, comment/note/link access with saved round trips. |
| 8 — **dc-971p.9** | Workbook Find/Replace/Go To, command context menus and trustworthy selection aggregates with keyboard access. |
| 9 — **dc-971p.10** | OneCalc: one main result, useful examples and input-first narrow layout, shared diagnostics, format/locale and fixture persistence/share; engine-free dependency checks continue to pass. |
| 10 — **dc-971p.11** | Atlas first among the other skins: actionable refusal/state overview and sheet navigation, complete reason access, optional diagnostics. |
| 11 — **dc-971p.12** | Ledger: accessible audit/recovery queue with reasons, source jumps, preserved filters and readable narrow rows; optional metadata stays optional. |
| 12 — **dc-971p.13** | Canvas: navigable dependency inspector with explicit omissions, range/blocker inspection, pan/zoom/reset and responsive details. Durable authoring cards remain a separately stated capability. |
| 13 — **dc-971p.14** | Tree: semantic keyboard outline, useful columns/search, persisted folds, spill/truncation access and shared source editing; hierarchy never changes formula scope. |
| 14 — **dc-971p.15** | Notebook: dependable block selection/editing and a blank-sheet creation/link flow, compact literals, cell-backed prose/order, bounded content projection and saved anchors/layout. |

Atlas and Ledger come before Canvas/Tree/Notebook because actionable calculation diagnostics
benefit every workbook immediately. Canvas next makes dependency reasoning useful; Tree then
organizes navigation; Notebook needs the largest change from inspection to authored presentation.
Do not present any of those later tasks as completed by the current views.

Coordinate OneCalc's feature work with **dc-wr9**, and reuse legacy interaction tests under
**dc-jnt.21/.22**. Document fidelity gaps connect to **dc-nn9**. Existing dc-09v, dc-6yw,
dc-gf0, dc-xib and dc-p96 are sources of implemented behaviour and fixtures, not evidence that
all Excel UI families are complete. Extend their routes instead of creating another evaluator,
graph, oracle, fixture generator or verdict system.

## Validation record

- `cargo xtask app smoke calc`: **71/71 cases in Chrome and 71/71 in Firefox**, exit 0.
  Log: `~/codex-runs/smoke-calc-initial.log`. The existing runner built the local wasm bundle,
  started the calculation worker, drove real input and checked the page's fault record.
- `cargo xtask app smoke onecalc`: **27/27 in each browser**, exit 0.
  Log: `~/codex-runs/smoke-onecalc.log`.
- Screenshot/exploratory runner: **63 local and 63 published captures** (21 scenes × 3 sizes
  per build), plus three setup/supplemental images. Both final runs exited 0; each app's page
  fault record was empty. Observations: `~/codex-runs/ui-{local,published}-observations.json`.
  External 2×2 TSV was successfully placed on Chrome's OS clipboard with browser permissions;
  Ctrl+V in the grid answered “Nothing is copied to paste”, leaving H1 empty. This confirms
  the private-clipboard limitation separately from a browser permission denial.
- Supplemental local check of `features.xlsx`: A1/B2 computed font weight 400, B5
  `white-space: nowrap`, K1 a plain div; after scrolling 300 px, the stored frozen first row
  was absent. `~/codex-runs/ui-supplemental.json` includes these observations and Chrome's
  accessibility tree; `local-calc-features-scrolled-1440x1000.png` records the scrolled view.
- `cargo xtask corpus --oracle --no-capture`: exit 0, **21/21 workbooks compared**,
  **19,284 matched / 20,030 formula cells (96.27%)**; **659 fail** (652 mismatched,
  6 refused, 1 failed/missing), **73 external-link** and **14 circular-rule** cells counted
  separately, zero awaiting capture. These separate populations are not passes; the pass-rate
  denominator is all 20,030 formula cells. Log: `~/codex-runs/ui-corpus.log`; detailed existing
  reports: `target/corpus/summary.md` and `.json`. This is a baseline measurement, not an
  improvement claim or a new Excel capture. No product code changed, so there is no numerical
  before/after delta to attribute to this review.
- `cargo xtask check`: exit 0 on Linux; new-text scan, provenance scan and workspace build
  passed. Only docs/tracker changed, so the command correctly ran **no package tests**.
  Log: `~/codex-runs/ui-check.log`. `br dep cycles --json` reports no cycles.
- Windows per-commit checks, full sweep, live Excel, native shell, assistive-technology user
  tests and touch/IME device tests were not run here. Browser smoke success does not cover the
  missing behaviours listed above. All fourteen implementation slices remain open.
