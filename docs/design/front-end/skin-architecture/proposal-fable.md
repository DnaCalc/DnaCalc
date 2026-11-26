# Skin architecture, proposal A: the skin interface and the code shape for a fresh DNA Calc UI

Author: Fable 5.2 (effort max), lane fa1, 2026-11-26. Bead dc-qttnl. Written alone: the other
proposal in this folder was not read. Tree read: `4120ba262`.

This is a proposal for the merge (dc-xnsw4). It designs one thing: the interface every DNA Calc
user interface is written against, and the code shape behind it. It changes no code.

How to read it. Section 0 is the whole design on one page. Sections 4 and 5 are the interface.
Sections 6 to 14 show what stands on it. Sections 15 to 17 are the code shape, the build order
and the hard parts. Section 18 lists what was not taken, with the reason for each.

## 0. The design in one page

1. **A skin is data. A view is code. The skin interface is what both stand on.**
   - The *skin interface* is one Rust contract crate, `calc-skin`. It holds targets, the
     selection, typed changes, the history, queries, events and the ports. Nothing above it can
     reach a host, an engine or a file.
   - A *view* is a compiled presentation engine written against that contract. There are five
     (Classic, Inspector, Notebook, Model map, App) and two tools (the Formula Explorer and the
     Forms maker).
   - A *skin* is a declaration, with no code in it: tokens, a keymap, the shell's layout,
     settings for each view, pages and named commands. The built-in looks are skins too. Skin 0
     is the built-in skin of the Formula Explorer. A file carries declarations only.
2. **Two host levels, two crates.** Level 0, the formula host (`calc-formula-host`): parse,
   complete, evaluate a trial, format, load add-ins. It holds no workbook and links no engine.
   Level 1, the workbook host (`calc-host`), is built on level 0 and adds cells, recalculation,
   history and the view part. The Formula Explorer is written against level 0 only, and its
   build contains no engine code.
3. **One workbook, one selection, one history.** They live in the host session, not in a view.
   A change of view changes only the presentation.
4. **Every edit is a typed change in a batch.** A change is a *model* change (Excel will read
   it) or a *presentation* change (it lands in the DNA view part, which Excel ignores). The
   label is computed from what a batch would write, never declared by the caller. Every batch
   can be previewed before it commits. That preview is the seam every view shows, and it is the
   same object an agent's proposal and a Formula Explorer "Apply" use.
5. **No view has code in the host.** Views read through one fixed set of neutral queries
   (cells, geometry, formula structure, dependencies, structure and readings, issues, sources,
   calculation state, objects, rows, history, view part). Any query can be watched. A new view
   needs no protocol change, no host change and no worker change.
6. **State is honest by type.** A view never receives a bare value: it receives a value with
   its provenance, inside an answer with a stamp (workbook, model revision, view revision,
   calculation generation) and a coverage (complete, or partial with counts). Observed
   evaluation, dependency explanation and value changes are three different answer types.
7. **Four kinds of participant, one grammar.** A person working through a view, an agent, a
   macro or automation client, and an installed component all name targets in the same address
   grammar and all change the workbook through the same batches. Each holds a port that stamps
   its origin and fixes how it may write: commit, propose, or only set its own bindings.
8. **One page grammar for everything that is laid out by declaration.** An App page, a panel in
   a skin, a control panel inside a notebook and the Inspector's bench are the same thing: a
   tree of standard components bound to addresses. The Forms maker edits that tree. An agent
   writes the same tree as JSON.
9. **View state that must survive lives in one custom XML part**, version 2 of
   `urn:dnacalc:view-manifest`: preferences, anchors, view documents, the travelling skin,
   readings, notes and findings, and the agent's change trail. Anchors are keyed to cells, the
   user's own names, tables and objects. No hidden names. Presentation changes are in the one
   history.
10. **One page, four carriers.** The same wasm page runs in a browser (host in a web worker), in
    WebView2 (host native), against `dnacalc serve` (host native on any system, over a loopback
    socket), and in tests (host in process).
11. **Code enters only as an installed, signed component**, sandboxed in an isolated browser
    frame, speaking a small versioned port: bound values in, intents on its own bindings out. Opening a file
    never runs anything from the file.
12. **The site and the app share one link grammar and one set of ids.** A link names an app, a
    source, an address, a view and a trial. Every command, function, refusal and view has a help
    id that is a page on the site. Both are generated from the tables the app runs on.
13. **Most of the page is thrown away and written fresh.** The contract replaces 72 intents, 77
    requests and 53 replies. `apps/dna-calc/src` (34,157 lines) is rewritten as a shell, a kit,
    a grid component and six small view crates. What is kept is what is proven by tests or was measured against
    Excel: the hosts, the pure grid logic, the tile stream, the formula field, Strand, and the
    scripted browser cases. There are 933 today. Those for Classic and the shell, at least 828
    of them, must pass on the new page before the old one goes.
14. **Build order:** contract and test kit; the Formula Explorer on level 0; level-1 serving,
    the runtime and the shell's frame, with a thin Classic as the contract's first client;
    Classic in full; instruments and Inspector; the view part and the Notebook; App, the Forms
    maker, skins and agents; the Model map; extensions and the views library for VBA. The new
    page is published beside the old one from the third step on. After that step a first
    working version of a view is hours of work. Classic is the exception, and section 17 says
    why.

```text
 page (one wasm bundle)                                   host
 ┌────────────────────────────────────────────────┐      ┌─────────────────────────────────────┐
 │ views: classic inspector notebook map app      │      │ calc-skin-host (level-1 serving)    │
 │        explorer (level 0)   forms maker        │      │   queries · watches · frames        │
 │ dna-kit  dna-grid  dna-formula-ui (level 0)    │      │   preview · commit · history        │
 │ dna-shell: mast deck bridge stage panels strip │      │   anchors · view part · proposals   │
 │ dna-skin-runtime: store, input queue, keymap   │      ├──────────────────┬──────────────────┤
 ├────────────────────────────────────────────────┤      │ calc-host (L1)   │ calc-formula-host│
 │ calc-skin: FormulaPort ⊂ WorkbookPort          │◀────▶│ engine, doc      │ (L0) fml, func,  │
 └────────────────────────────────────────────────┘ wire │                  │ add-ins          │
   carriers: web worker · WebView2 · loopback socket · in process                              │
                                                         └─────────────────────────────────────┘
```

## 1. Ground

### 1.1 Rulings applied

From `DECISIONS_20261126.md` (rulings 1 to 14, called D1 to D14 here) and the README's
agreements. They override every source, and this proposal applies them as follows.

| Ruling | Effect here |
|---|---|
| D2 standalone Formula Explorer, early | The Explorer that exists stays published as the teaser. Level 0 is the first vertical slice rebuilt (section 16, stage 1). |
| D3 RTD clock for our ticked models; macro-driven ticks also valid | Ticks are ordinary source events and calculation generations. The contract has no tick concept of its own (4.7, 7.4). |
| D4 undo across a macro run: design for it, do not promise it | History entries carry reversibility and a run group from the first version. The checkpoint restore is a later stage with an experiment list (17.1). |
| D5 notes and findings in customXml now, visible mirror optional | `notes` section of the view part (12.2). The mirror is an explicit model change, not built first. |
| D6 the notebook works exactly like Jupyter; no read-mode presentation | The Notebook view has one presentation and Jupyter's cell model and keys (7.3). Read-only is a capability of how the file was opened. A reader-style page is an App page made with the Forms maker (7.5, 7.6). |
| D7 FerroUI not now | The page stays Leptos in WebView2 and the browser. The contract and the runtime are toolkit-neutral, so this stays reversible. |
| D8 a new website hook | The hook itself belongs to the book. Section 14 gives what it needs from the architecture: links both ways and shared ids. |
| D9 skin architecture is central | This document. |
| D10 write fresh, throw away large chunks, no micro-step migration; publish for feedback | Section 15 lists what is deleted and the one commit where the old page goes. The new page is published beside the old one as each stage lands (16). |
| D11 Classic uses Excel's key-tip letters exactly | Key tips are a path table of data, apart from the Deck's look (8.3). |
| D12 Ctrl+Alt+1…5 switch views unless Excel binds them | Shell keymap, checked against the Excel profile table (8.3). |
| D13 no hidden names as anchors | View part v2 anchors (12.3). The v1 hidden locator names are retired. |
| D14 the agent's change trail is kept in the file, can be turned off | `trail` section of the view part (12.2, 10.4). |
| README: one workbook, one selection, one undo history | Host-owned (4.2, 4.5). |
| README: every edit outside the grid is labelled model or presentation before commit | `Preview.kind`, computed (4.4). |
| README: lens is read-only; "Rewrite as LET" is an explicit edit | `Present` is a level-0 query; a rewrite is a model change with a preview (5.1). |
| README: per-sheet view is a preference with a badge | `ViewPreference`, never applied by a tab click (12.2). |
| README: map is a reader first; boxes are cells with names on a visible sheet | The map reads `Structure` and `Deps`; creating a box is a model change through the placement query (7.4). |
| README: Inspector tools are panels in every view | Instruments are components in the kit; the Inspector view arranges them (7.2). |
| README: skins in a file are declarations; coded components are installed and signed | Sections 8 and 9. |
| Owner rule: the engine does no more than Excel; views use only what Excel exposes | Every model change maps to an Excel operation (4.4). Every query can be answered from what Excel exposes, except the host's own calculation record, which is typed as such and may be absent (4.7). |

### 1.2 The code as it is

Facts from the tree at `4120ba262`. Line counts are `wc -l` over each package's `src`.

| Piece | Where | Size | State |
|---|---|---|---|
| Host session | `crates/calc-host` (`HostSession` `session.rs:443`, `SkinIntent` `:145`, `WorkbookCommand` `actions.rs:205`) | 51,264 | One session, one transaction per edit, undo by inverse edits or whole-root restore (`history.rs:56`), deltas to subscribers, an intent log with replay. Portable. The selection, `ActiveCell` and `ActiveWindow` that VBA and COM see are this session's (`automation.rs`, `automation/window.rs`). |
| Page | `apps/dna-calc/src` | 34,157 | A Leptos page: a `Controller` struct of about 60 signals (3,201 lines, 7,018 with `controller/` and the input adapter), six skins (grid 8,948 lines; notebook, atlas, tree, canvas, ledger 3,729), and one file per dialog family (11,038), most of which talk to the wire themselves. Commands are buttons in the title bar; there is no command band and there are no key tips. |
| Page model | `apps/dna-calc-model` | 18,627 | Portable, no DOM. 12,744 lines of logic (key grammar, render plan, geometry, viewport, tile mirror, print layout, drawn shapes) and 5,883 of wire types and state (`projection.rs` is 2,402). The edit session is a struct with a `point_mode` flag (`state.rs:54`); Excel's Ready, Enter, Edit and Point are not modelled as states. |
| Worker | `apps/dna-calc-worker` | 17,962 | `server.rs` is 10,027 lines: request dispatch, intent translation, and one projection per skin (`Notebook`, `Canvas`, `Ledger`, `Atlas`, `Outline`). |
| Wire | `dna_calc_model::projection` | | 72 `Intent`, 77 `Request`, 53 `Reply`, 8 `Event` variants (counted by script on this tree). |
| Desktop | `apps/dna-calc-desktop` | 3,304 | The page runs unchanged in WebView2. A script replaces `Worker` with a bridge to a native `Server` (`web/desktop-bridge.js`). Tile bytes cross as base64 in a string. Calculation runs on the window's thread in 6 ms slices of an 8 ms tick (`app.rs`). |
| CLI | `apps/dnacalc` | 1,590 | `recalc`, `get`, `set`, `diff`, `run`, and `serve`: the same page over a loopback WebSocket to a native `Server`, on any system (`serve.rs`). |
| Formula Explorer | `apps/dna-onecalc`, `-desktop` | 9,846 and 5,481 | Its own `Session` (`session.rs:193`: one formula, inputs keyed by cell, calc-fml and calc-func only), its own port (32 requests, 17 replies), its own shell and an add-in surrogate process. 5,940 of the 9,846 lines are pure logic outside the port. |
| Formula UI | `apps/dna-formula-ui` | 4,001 | The formula field and value display both apps share. Reaches no engine and no host. It depends on `dna-calc-model`. |
| Tokens | `apps/dna-strand` | 1,271 | Three themes, two densities, 23 colour tokens per theme, and contrast tests over 61 sanctioned pairs (`contrast.rs:77`). |
| View manifest | `calc-doc` load/save `manifest.rs`, `calc-doc-model/src/layout.rs` | 1,989 | Version 1: blocks located through hidden names `_dna.block.<hex>`, placements for four views. Only the notebook writes it. Unknown fields are preserved byte for byte. |
| Macros | `calc-host-vba`, `calc-macro-security` | | `run_procedure` (`command.rs:116`) is reached only from `dnacalc run`; the page cannot start a macro. Levels are `Off`, `Guarded`, `Unguarded`. A wire field for an undo group exists (`projection.rs:839`); it is always empty and nothing reads it. |
| Model clock | `apps/dna-calc-worker/src/model_clock.rs` | 185 | An in-process RTD provider `DnaCalc.Clock` for ticked models (dc-hul4). It has tests and no caller outside them. |
| Evidence | `apps/dna-calc/tests/smoke/cases.rs` | 18,132 | 933 scripted browser cases (`Case {` blocks), run by `tools/xtask-harness` over WebDriver in Chromium and Firefox. They address the page through `data-dnacalc` role attributes and a render-plan hook; about 70 selectors name CSS classes, element ids or child positions. 57 cases switch to one of the five non-grid skins and 105 name one of them (counted by script). |
| Classic parity | `docs/plan/UI_REVIEW.md` | | 31 rows: 10 Done, 21 Partial (counted on this tree). |

Four properties of this code decide the shape below.

- **The host side is sound and the page side is where the tangle is.** The 17-harvest checked the
  tile route: publications are atomic per subscription, slow subscribers coalesce into one
  reset, retries never run twice (`CommandJournal`, `envelope.rs`), a late event to a retired
  view does nothing. None of that needs redesign. What makes a new view take days is on the page
  and in `server.rs`: the controller is one struct every skin shares and extends, each skin has
  its own worker projection and its own request and reply, and every dialog family talks to the
  wire itself.
- **The document model is persistent.** `ModelRoot::pin` (`calc-doc-model/src/root.rs:289`)
  retains an exact authored root with shared pages, so a checkpoint copies nothing when it is
  taken. Preview, proposals and the macro-run design (17.1) all rest on this.
- **Level 0 already exists twice.** The Formula Explorer's `Session` is a formula host, and the
  DNA Calc page already calls calc-fml and calc-func for completion, diagnostics and number
  formats. They are not one thing yet.
- **The four carriers already exist.** Web worker, WebView2 bridge and loopback socket all carry
  the same messages to the same `Server`, and a headless transport runs it in process. The
  rebuild changes what the messages say, not how they travel.

### 1.3 What the designs ask of the system

The four Pro 19 answers give 34, 50, 22 and 32 C-needs (GPT-6 Pro, Fable, Carbon, Neon). They
overlap heavily. Appendix A groups them into 29 needs and maps each group to its sources and to
the contract item that meets it. The groups that shape the interface most:

- identity that survives edits and reopening (Pro C-1, Fable C-43, Carbon C-15);
- value state and revision on everything shown (Pro C-3, Fable C-2 and C-3, Carbon C-11);
- being told what changed, not re-reading (Pro C-4, Fable C-4, Neon C-5);
- staged opening with empty, not yet loaded and failed kept apart (Pro C-5, Fable C-1, Neon C-1);
- dependencies summarised by range, block and name, with exact membership behind a summary
  (Pro C-7 and C-8, Fable C-8 to C-11, Neon C-11 and C-14);
- one preview-and-apply contract for every editor, atomic, refusing stale bases (Pro C-11 and
  C-12, Fable C-17 and C-18, Neon C-6);
- one history across model and authored presentation, with attribution and irreversibility named
  (Pro C-13 and C-33, Fable C-40 and C-41);
- trial evaluation in an explicit caller and input context, with no workbook (Pro C-15, Neon C-9,
  Carbon C-9);
- observed execution kept apart from dependency explanation and from value differences (Pro C-20
  and push-back 2);
- a preserved, versioned view part with detection of loss (Pro C-26 and C-27, Fable C-44);
- estimates before expansion, and cancellation (Pro C-31 and C-32, Fable C-50).

The Pro 21 scenes add four needs the Pro 19 lists do not state: a notebook cell's formula and
its output name both live in ordinary cells and names (NC01, NC06); a binding carries both a
stable identity and an exact address, and both must match before it is used (U01, M05); a
proposal is reviewed and applied by unit, with the effect worked out for the chosen subset
(S14 to S16); and a print or a preview never consumes a live observation (O03, I02).

## 2. Vocabulary

| Term | Meaning | Kind |
|---|---|---|
| Skin interface | The contract crate `calc-skin`: the only thing page code and hosts share. | Rust types and ports |
| Host level 0 | The formula host. No workbook, no engine. | `calc-formula-host` |
| Host level 1 | The workbook host, built on level 0. | `calc-host` served by `calc-skin-host` |
| View | A presentation engine: code that draws one kind of presentation and turns gestures into commands. Five views, fixed. | A crate implementing `View` |
| Tool | A view that is not a workbook presentation: the Formula Explorer (level 0) and the Forms maker (edits pages). | A crate or module |
| Skin | A named declaration, no code: tokens, keymap, shell layout, view settings, pages, named commands. What a person or an agent makes. | JSON, schema `dna.skin/1` |
| Skin 0 | The built-in skin of the Formula Explorer: level 0, one view. The standalone build loads only this skin. | A built-in declaration |
| Component | A UI unit with a typed binding contract (value, input, data frame, chart, text, trace and the rest). Views and pages are built from the same set. | Code in `dna-kit`, or an installed extension |
| Page | A declared tree of components bound to addresses: an App page, a panel, a notebook control panel. | JSON, schema `dna.page/1` |
| View document | A view's authored state for one workbook (a notebook's cells, a map's layout, an App's pages). | JSON in the view part |
| View part | The one custom XML part DNA Calc keeps in a workbook. | `urn:dnacalc:view-manifest` version 2 |
| Anchor | A stored reference from the view part to a cell range, name, table or object, with a fingerprint for finding it again. | An entry in the view part |
| Participant | Whoever changes or reads the workbook: a person through a view, an agent, a macro or automation client, a component. | An origin stamped by a port |
| Port | A participant's handle on the contract. It fixes the level, the origin and how writes happen. | `FormulaPort`, `WorkbookPort`, the component port |
| Carrier | What moves envelopes between a port and a host. | Web worker, WebView2, socket, in process |
| Shell | Everything every view shares: Mast, Deck, Bridge, stage, panels, Strip, sheet tabs, palette, dialogs, trust cards. | `dna-shell` |

The current code calls its six presentation modules "skins". In this design those are views.
"Skin" is kept for what the owner's scope page says a skin is: a manifest. "The skin interface"
keeps the owner's phrase for the contract all of it stands on.

## 3. Layers and rules

```text
 7  compositions      apps/dna-calc (page), dna-calc-worker, dna-calc-desktop,
                      dna-onecalc, dna-onecalc-desktop, dnacalc (CLI)
 6  views and tools   dna-view-classic, -inspector, -notebook, -map, -app, -explorer
 5  shell             dna-shell
 4  kit               dna-kit, dna-grid, dna-formula-ui, dna-strand
 3  runtime           dna-skin-runtime            (portable, no DOM, no toolkit)
 2  contract          calc-skin                   (portable, types and ports only)
 ───────────────────── the wire: one envelope, any carrier ─────────────────────
 1  serving           calc-skin-host              (portable)
 0  hosts             calc-formula-host (level 0), calc-host (level 1)
    below             calc-formula-kernel, calc-engine, calc-doc, calc-fml, calc-func,
                      calc-value, the add-in hosts
```

Rules, each checked mechanically in `tools/xtask/architecture.json` (the existing checker):

1. `calc-skin` depends on calc-value, calc-macro-security (three small enums), serde and
   serde_json. It depends on no host, no engine, no codec and no UI toolkit. Where the contract
   needs a host's notion (a work budget, the calculation environment) it has a plain type of
   its own, and the serving crate converts.
2. A view crate depends on `calc-skin`, `dna-skin-runtime`, `dna-kit`, `dna-grid`,
   `dna-formula-ui` and `dna-strand`. It depends on no host, no serving crate, no carrier, no
   shell and no other view.
3. `dna-view-explorer`, `dna-formula-ui` and `dna-onecalc` have no calc-engine, calc-host or
   calc-skin-host in their closure. This is today's rule for `dna-onecalc`, moved to where
   level 0 now is.
4. `calc-formula-host` has no calc-engine and no calc-host in its closure. `calc-host` depends on
   it and never copies what it does.
5. The page of DNA Calc has no calc-doc, calc-host or calc-engine in its closure, as today. It
   reaches level 0's pure part through `calc-formula-kernel` (5.1).
6. Only a composition names views, panels and components. The shell and the kit receive
   registries.
7. A view holds no engine handle. It holds a port, and the port's changes are Excel's. This is
   the guarantee the owner's scope page states: a skin cannot change a calculation.

## 4. The contract

All types below are in `calc-skin`. They are plain data with serde, and they are the wire. The
listing is the proposal; names can change in the merge, the shape is the point.

### 4.1 Targets and addresses

One grammar for what can be pointed at. A person types it in the address field and Go To, a
macro passes it, a skin binds to it and an agent sends it (Strand tenet 7).

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    /// One or more rectangles, all on one sheet, with Excel's multi-area rules: `Monthly!H13`.
    /// A `GridArea` carries its sheet.
    Cells { areas: Vec<GridArea> },
    /// A spill by its first cell: `Monthly!C21#`.
    Spill { anchor: CellKey },
    /// A defined name with its scope: `ServicesShare`, `Sheet1!Local`.
    Name(NameRef),
    /// A table or part of one: `Orders`, `Orders[Amount]`, `Orders[@Date]`.
    Table { table: TableRef, part: TablePart },
    /// A chart, shape, control, picture or pivot by its Excel name: `Loan!"Balance"`.
    Object { sheet: SheetId, name: String },
    Sheet(SheetId),
    Workbook,
    /// A presentation item: `view:notebook/analysis/c12`. Never a model address.
    Item(ItemRef),
}

pub struct ItemRef { pub view: ViewId, pub doc: DocId, pub path: String }
```

- `Target` is the session form (ids). `Address` is the text form. `Resolve { text, relative_to }`
  turns text into a target; `Display { target, notation }` turns a target into A1 or R1C1 text.
  Both are queries, so every view, the palette, macros and agents resolve the same way.
- A *block* (a run of like formulas with its labels) is not a target kind. It is a reading over
  `Cells`, shown with its label. A user makes it a model object by pinning a name, which is a
  model change.
- A view item that stands for model content shows the model address. A pure presentation item
  shows its view path, and the formula bar is empty.
- Today's `dna_calc_model::Target` (`command.rs:78`) has five of these kinds, with names by id
  and objects and layout items by bare strings, and without tables, spills and the workbook.
  This one replaces it.

### 4.2 The one selection

```rust
pub struct Selection {
    /// The model object selected. Present whenever a workbook is open.
    pub target: Target,
    /// Excel's ActiveCell, when the target has cells.
    pub active: Option<CellKey>,
    /// The presentation item holding focus in the view that made the selection, if any.
    pub item: Option<ItemRef>,
    /// The grouped sheets, when more than one is selected. An edit then writes to all of them.
    pub sheets: Vec<SheetId>,
}
```

- The selection belongs to the host session, as it does today (`HostSession::selection`). It is
  the same object `Application.Selection` and `ActiveCell` answer from. A macro's `Select`
  moves it and every view follows.
- A view sets it with `port.select(selection)` and reads it through the runtime's selection
  handle. Selecting is not a history entry. Each history entry records the selection before and
  after, so undo restores where the edit was made (the host does this today, `history.rs:70`).
- A move that depends on what the cells hold (Ctrl+arrow to the edge of the data, End, Go To
  Special) is `port.navigate(by)`. The host resolves it, as it resolves `Navigate` today, and
  answers with the new selection.
- **The page moves first and the host confirms.** Every `select` and `navigate` the page sends
  has a sequence number. The host's selection event says what caused it: an echo of that
  sequence, a macro, an agent, an undo. The runtime drops an echo older than its latest move,
  and takes anyone else's move as it comes. So the page can tell its own echo from a macro's
  `Select`, which is what Classic's modes and type-ahead need (17.10).
- A view shows the selection through `Mounted::locate` (6.2): exact, inside a coarser item (the
  Strip says which), or absent. Absent offers "Reveal in Classic" and "Add to this view". It
  never substitutes another selection.
- Scroll, camera and folds are not selection. They belong to a view and a sheet, and they are
  kept per user, outside the file and outside the history (12.6). The one exception is
  Classic's own: a sheet's panes and zoom are Excel's sheet-view settings and stay in the file
  (7.1).

### 4.3 Stamps, provenance, coverage

```rust
pub struct Stamp {
    pub workbook: WorkbookId,       // changes when the workbook is replaced
    pub model: Revision,            // advances when a batch changes what Excel reads
    pub view: Revision,             // advances when a batch changes the view part
    pub calc: CalcGeneration,       // advances with every published calculation
}

pub enum Provenance {
    Empty,
    Literal,                        // typed into the cell
    Calculated,                     // the engine calculated it and it stands
    AsSaved,                        // the value the file stored; nothing here calculated it
    Stale(StaleCause),              // no value stands at this revision; the last one is shown
    Refused(NotModelled),           // a typed refusal naming its owning work package
}
pub enum StaleCause { ManualMode, Calculating, CalcFailed, Cancelled, SourceUnavailable }

/// A view never receives a bare value.
pub struct Shown<T> { pub value: T, pub provenance: Provenance }

pub enum Coverage {
    Complete,
    Partial { have: u64, of: Option<u64>, why: PartialWhy },   // loading, windowed, over budget, failed
}

pub struct Answer<T> { pub stamp: Stamp, pub coverage: Coverage, pub body: T }
```

- These are today's `Provenance` (`projection.rs:9`) and stale causes with one rename (file cache
  becomes "as saved", the word every design uses) and one addition (the cause of staleness is on
  the value).
- **Two revisions, not one.** Today one authored revision advances for a cell edit and for a
  moved notebook block alike, because the layout is part of the model root. Splitting it makes
  three claims checkable instead of asserted: a presentation change leaves `model` where it was;
  a view switch leaves both where they were; "saved, layout edited" (Fable R-34) is `model`
  equal to the saved one and `view` ahead of it. The host change should be small (an estimate,
  not a measurement): the root records the revision at which its cells, definitions or sheets
  last changed.
- **How these nest.** A watch delivers an `Update`. A fresh update holds an `Answer`: a stamp, a
  coverage and a body. Inside a body, every value a cell holds is a `Shown`. A body that is an
  explanation or a record, not a reading of the file, is wrapped in `Evidence` (4.7). Three
  different things can be out of date and have three names: a cell's value that does not stand
  (`Provenance::Stale`), a watch whose answer no longer holds (`Update::Outdated`), and a batch
  made against an older state (`Refusal::Conflict`).
- An Excel error is a value (`#REF!` stays `#REF!`). A refusal, a missing provider or a load
  failure is never shown as an Excel error code (Pro R-9).
- The kit's `Value` and `DataFrame` components draw the provenance marks. A new view gets honest
  state without writing any of it.

### 4.4 Changes, batches, preview, commit

```rust
pub enum Change {
    /// Something a user could do in Excel. Lands in Excel's own parts.
    Model(ModelChange),
    /// Lands in the DNA view part. Excel ignores it. Can never change a value.
    Presentation(PresentationChange),
}

pub struct Batch {
    /// The model revision the caller saw. With a base, the batch is refused when anything it
    /// writes, or anything listed in `expects`, was changed after that revision.
    pub base: Option<Revision>,
    pub expects: Vec<Target>,       // what the caller read to decide this change
    pub label: Option<String>,      // "Fill C7:N7"; the host names it otherwise
    pub group: Option<GroupId>,     // ties this batch's entry to a proposal or a macro run (4.5)
    pub changes: Vec<Change>,
}
```

**Model changes** are Excel's commands over targets. The vocabulary is the one calc-host already
has, re-expressed over `Target`: the edit variants of `SkinIntent` (enter, enter over the
selection, edit, copy or move from one range to another, axis insert and delete) and the 53
variants of `calc_host::WorkbookCommand` (`actions.rs:205`: format, names, tables, filters,
sort, sheets, outline, panes, charts, shapes, sparklines, pivots, validation, conditional
formats, page setup, review, paste, replace). Four are added because the views need them and
Excel can express them: a run edit, a single reference replaced, a name renamed everywhere,
and Excel's scenarios, which calc-host does not model yet.

```rust
pub enum ModelChange {
    Enter { at: Target, text: String },                 // read as Excel reads a cell entry
    EnterRun { run: Target, text: String },             // one relative formula over a run, one step
    ReplaceReference { at: CellKey, occurrence: u32, with: Target },   // the map's rewire
    RenameName { name: NameRef, to: String },           // everywhere it is used
    Scenario { .. },                                    // Excel's scenarios: add, apply, delete
    // … the existing vocabulary, by family:
    Clear { .. }, Fill { .. }, Paste { .. }, Move { .. }, Insert { .. }, Delete { .. },
    Format { .. }, Name { .. }, Table { .. }, Sort { .. }, Filter { .. }, Sheet { .. },
    Chart { .. }, Shape { .. }, Pivot { .. }, Validation { .. }, Conditional { .. },
    PageSetup { .. }, Review { .. }, Settings { .. },
}
```

Rule: a model change that a person could not make in Excel does not exist. A gesture that has no
Excel meaning (a wire dropped on `=A*B+C` with nowhere to go) is refused with the reason and the
nearest thing Excel can express.

This is the one public vocabulary. The page's own `WorkbookCommand` (69 operations,
`apps/dna-calc/src/workbook_command.rs`) and the wire's 72 intents both go. calc-host keeps its
internal command type for now, and `calc-skin-host` holds the one translation (section 19, point 1).

**Presentation changes** edit the view part. They are generic, so a new view needs no host code:

```rust
pub enum PresentationChange {
    /// Edit a view document. `at` is the document's revision the patch was made against; a
    /// patch against an older one is refused as a conflict.
    Doc { doc: DocId, at: Revision, patch: Vec<PatchOp> },
    CreateDoc { view: ViewId, doc: DocId, scope: DocScope, body: Value },
    RemoveDoc { doc: DocId },
    Anchor(AnchorChange),                               // add, rebind, remove
    Prefer(ViewPreference),                             // workbook and per-sheet view preference
    Skin(SkinChange),                                   // the declaration that travels in the file
    Note(NoteChange), Reading(ReadingChange), Trail(TrailSetting),
}

pub enum PatchOp {
    Put    { path: Path, value: Value },
    Remove { path: Path },
    Insert { path: Path, index: u32, value: Value },
    Move   { from: Path, to: Path, index: u32 },
}
```

The host applies a patch to a JSON tree it does not interpret, and undoes it by restoring the
earlier root, which shares everything the patch did not touch. Undo therefore works for a view
the host has never heard of.

**Preview** is the seam. One call, one answer, the same for a notebook cell, a map wire, a paste,
an App input, an Explorer "Apply" and an agent's proposal:

```rust
pub struct Preview {
    pub base: Stamp,
    pub writes: Vec<Write>,            // every cell, name, object and setting, old and new, in Excel's syntax
    pub presentation: Vec<DocWrite>,   // what changes in the view part
    pub kind: ChangeKind,              // Model, Presentation or Both: computed from the writes
    pub refusal: Option<Refusal>,      // protected, spill child, validation stop, stale base, not modelled
    pub warnings: Vec<Warning>,        // overwrites a formula inside a run; would close a loop; would
                                       // block a spill; touches grouped sheets or hidden rows
    pub effect: Option<Effect>,        // asked for separately: values that would change
}

pub enum PreviewDepth { Writes, Effects { budget: WorkBudget } }
```

- `kind` is never the caller's claim. "Presentation only; 0 cells, formulas or names" (Pro R-60)
  is read off `writes` being empty. The test that goes with it: the seam's list equals the change
  journal after commit (Fable R-35).
- `Writes` should cost a prepare without a commit. calc-host already prepares the document's
  and the engine's share of an edit against one base before it commits either
  (`command.rs:1088` to `:1176`), and a preparation that is dropped leaves nothing behind;
  preview stops before the commit.
- `Effects` needs a trial calculation. It is budgeted and can come back partial. It is the
  expensive depth and is named in 17.4.
- **The seam has two forms, and the preview decides which.** The *label* is shown in place
  before the gesture commits: the kind of change and its destination ("Model change ·
  Loan!B3"). It asks for nothing, and Enter commits. A bound input in an App, a notebook cell
  whose backing cell is already known, and a formula typed in the Bridge get the label. The
  *review* lists the writes and waits for Apply. It is used when the preview has a warning or
  a refusal, when the batch creates model content at a place the user has not seen (a
  placement), when it writes more than the item on screen stands for (a run edit, a rewire,
  a rename), and for every proposal. Typing into a Classic cell needs neither: the cell is its
  own preview, except in the cases Fable R-25 lists (overwriting a formula inside a run, pasting
  into hidden rows, writing to grouped sheets), which get the review.

**Commit:**

```rust
pub struct Receipt {
    pub entry: Option<EntryId>,        // the history entry made, if the batch changed anything
    pub stamp: Stamp,
    pub kind: ChangeKind,
    pub selection: Selection,
    pub notes: Vec<Notice>,            // a validation warning Excel would show, a repair, a fitted spill
}
```

A batch is atomic: all of it commits as one transaction and one history entry, or none of it. A
batch may mix kinds. Creating a notebook cell is one batch: the formula in its backing cell
(model) and the cell's place in the notebook (presentation). calc-host has this shape today for
one case (`SkinIntent::EnterCellAndLayout`, `session.rs:157`). A commit carries a command
identity, and the host's journal never runs one identity twice (kept as it is, `envelope.rs`).

**Operations** are session verbs that are not edits and make no entry of their own, except where
noted:

```rust
pub enum Operation {
    Undo, Redo, Revert(EntryId),                       // each writes through the history
    Calculate { scope: CalcScope, selection: CalcSelection }, CancelCalculation,
    Open(OpenRequest), New(TemplateRef), Save(SaveRequest), Close,
    CopyToClipboard { target: Target, cut: bool }, PreparePaste(PasteSource),
    RunMacro { procedure: String, args: Vec<CellValue> },
    EnableMacros(MacroEnable),                         // this workbook's project
    Refresh(RefreshScope),
    Decide { proposal: ProposalId, accept: Vec<UnitId>, reject: Vec<UnitId> },
    TryOn(Declaration),                                // hand a skin or a page to the shell (10.2)
    Checkpoint(CheckpointRequest),                     // take, list, restore (17.1)
    View(ViewRequest),                                 // a macro's or a client's request to the shell (13.2)
    Automate(AutomationBatch),                         // an Office.js batch, as today
}
```

Loading an add-in and setting the macro security level are level-0 operations (4.9): they
belong to the formula session, with or without a workbook. Copying to the clipboard changes
nothing; the paste that follows is the model change.

Navigation (selection, scrolling, switching view, opening a panel) is neither a change nor an
operation. It goes on the shell's Back and Forward list.

### 4.5 The one history

```rust
pub struct HistoryEntry {
    pub id: EntryId,
    pub label: String,                 // "Entered Inputs!C5", "Moved box Store", "Macro: Roll_Month_End"
    pub origin: Origin,
    pub kind: ChangeKind,              // Model, Presentation, Both
    pub view: Option<ViewId>,          // where a presentation change was made
    pub targets: Vec<Target>,
    pub cells: u64,
    pub reversible: Reversibility,
    pub state: EntryState,             // Done, Undone
    pub marks: Vec<EntryMark>,         // Unreviewed (applied by an agent, not yet kept), Interrupted
    pub group: Option<GroupId>,        // a macro run, an agent proposal
}

pub enum Origin {
    User { view: ViewId },
    Agent { agent: String, proposal: ProposalId },
    Macro { procedure: String },
    Component { id: String, view: ViewId },
    Automation,
}

pub enum Reversibility {
    Reversible,
    /// The document can be restored. What is listed is not undone.
    DocumentOnly { outside: Vec<OutsideEffect> },
    Irreversible(IrreversibleReason),
}
```

- **One ordered history per workbook**, in the host, for model and authored presentation changes
  alike. Ctrl+Z in any view undoes the most recent entry, whatever view made it. The Strip names
  what was undone, its kind and its view, with a link to it. Undo never switches view.
- The origin is stamped by the port, not written by the caller. A view's port stamps
  `User { view }`; the agent's port stamps `Agent`; macro and automation entries are stamped by
  the host's own ingress (`CommandIngress`, `session.rs:345`). A view cannot attribute its edit
  to someone else.
- A fill, a paste, a run edit, an accepted proposal unit and a macro run are one entry each.
  The commits inside a macro run are merged into the run's one entry. The units of a proposal
  stay separate entries that share a group. Presentation changes coalesce: one drag is one
  entry.
- `Revert(entry)` writes a new entry that restores what an older one changed. It refuses, with
  the conflicts, when later work touched the same cells.
- Draft text inside an editor has its own local undo until it commits. Then Ctrl+Z acts on the
  history.
- Excel's own rules stay where Excel has them. A change Excel does not put on its undo stack is
  entered as `Irreversible` with that reason, not silently made undoable. The one deliberate
  addition is the macro run (17.1).

One history, not a model history plus a presentation history per view. The Fable Pro 19 answer
argued for the second, so that Ctrl+Z in the grid could not undo a box drag that the grid cannot
show. The owner's ruling is one history, and the objection is met another way: the Strip always
says what was undone and where, and an undone presentation step never moves the user.

### 4.6 Queries, watches, frames

A query is a typed question with a typed answer. Every view reads through the same set. There is
no query that belongs to one view.

```rust
pub trait Query: Serialize + DeserializeOwned + 'static {
    type Answer: Serialize + DeserializeOwned + 'static;
    const NAME: &'static str;
}
pub trait FormulaQuery: Query {}       // level 0
pub trait WorkbookQuery: Query {}      // level 1
```

Level 1 queries:

| Query | Answers | Needs it |
|---|---|---|
| `Summary` | Name, state word, sheets, calculation mode, macro state, open report, capabilities | shell |
| `Census { scope }` | Counts without a calculation: formulas, constants, errors by kind, used extents, objects | every view's `fit` |
| `Inventory { kinds }` | Sheets with visibility and protection, names, tables, objects, pivots, scenarios, links, macro modules, add-in functions in use; complete or partial | shell, Inspector, map |
| `Cells { sheet, areas, detail }` | Cells with input, shown text, format and provenance. Watched, it is the tile stream | all |
| `Geometry { sheet }` | Axis sizes, hidden, outline, merges, panes, zoom | Classic, data frames |
| `Formula { at }` | Structure with spans, each reference resolved to a target, R1C1 form, run membership | lens, map, notebook |
| `Deps { of, direction, depth, group, limit }` | Edges grouped by cell, run, block, name or sheet, with kinds (direct, through a name, through a table, runtime-selected, unresolved) and counts | Inspector, map |
| `Structure { scope, level }` | Runs with their exceptions, blocks with labels, tables, and readings with evidence and state | notebook draft, map, App draft |
| `Issues { scope }` | Errors grouped by root with dependent counts; warnings by kind | Inspector, tabs |
| `Sources` | External links, RTD topics, volatile functions in use, add-in functions; state, last value and time | Inspector, Strip |
| `CalcState` | Mode, iteration settings, status and progress, cycle members, last duration | Strip, map |
| `CalcEvents` | The host's own record of a calculation, as an event list: today's 12 kinds. Absent on a host that keeps none | Calculation instrument |
| `Changes { generation }` | Cells whose value changed in one publication, old and new | ripple, Inspector |
| `Explain { at, part }` | Step-by-step evaluation in workbook context; the first sub-expression that produced an error | Trace |
| `Contributions { at }` | For criteria functions and lookups: rows that passed, rows failing one test only | Trace on Sales |
| `Rows { source, filter, sort, window }` | Row windows with stable source targets; AutoFilter state untouched | data frames |
| `Objects { sheet }` | Charts as series data and formatting, shapes, pictures, controls with linked cells, sparklines | Classic, notebook, App |
| `Rules { at }` | Validation, conditional formats, protection, editability | all editors |
| `Find`, `Special`, `Stats`, `Resolve`, `Display` | Find and Go To Special, status aggregates, address resolution | shell |
| `Placement { near, shape }` | Where new model content would go by the placement rule | notebook, map |
| `TrialFor { at }` | A level-0 trial for one cell: formula, caller, inputs snapshot, revision | Explorer |
| `Print { what, setup }` | Pagination | printing |
| `History`, `SelectionNow` | The entries; the selection | shell |
| `ViewPart { section }` | Preferences, anchors with their states, documents, notes, trail, readings, requirements | all views |
| `Macros` | Modules and procedures (read without running), what each control and event calls | Inspector, trust cards |
| `Proposals` | Pending agent proposals and their units | review panel |

Many of these exist today as requests (`Geometry`, `Names`, `Find`, `SelectionStats`,
`TraceFormula`, `EvaluatePart`, `Charts`, `Shapes`, `Controls`, `PivotTables`, `Validation`,
`PrintPreview`). `Deps` grouped, `Structure`, `Issues` by root, `Sources`, `Contributions`,
`Rows`, `Placement` and `Census` are new host work, and section 16 places each. Appendix B maps
the old messages to these.

`Structure` carries the most and is the least built, so its answer is fixed here:

```rust
pub enum StructureItem {
    Sheet { sheet: SheetId, used: Option<GridArea>, counts: Counts },
    Table { table: TableRef, area: GridArea, columns: Vec<ColumnInfo> },
    /// Formulas that are the same in relative terms, with the members that differ.
    Run   { area: GridArea, formula_r1c1: String, members: u64, exceptions: Vec<CellKey> },
    /// A rectangle that belongs together: its label cells, its runs, its role.
    Block { area: GridArea, label: Option<Label>, role: BlockRole, runs: Vec<GridArea> },
    Name  { name: NameRef, refers_to: Option<Target> },
    /// What the program infers and a person may confirm or correct (12.2).
    Reading { id: ReadingId, kind: ReadingKind, over: Target, state: ReadingState,
              evidence: Vec<String> },
}
pub enum BlockRole { Inputs, Calculation, Outputs, Text, Unclassified }
pub enum ReadingState { FromWorkbook, Suggested, Confirmed { by: String }, NeedsReview }
```

`level` picks how far down the answer goes: sheets; then tables, blocks and names; then runs.
Sheets, tables, names and runs are facts of the file. Blocks, roles and readings are
inferences, and the answer says which is which. It grows over the build: runs and tables
first, roles for inputs and outputs next, levels for the map last (16).

**Only what Excel exposes.** Each query above can be answered from what Excel itself exposes:
cells, formulas, names, tables and objects; precedents and dependents; values before and after a
calculation; calculation settings; validation and formats. Runs, blocks, error roots and
contributions are worked out from those and are labelled as readings or as explanations. The one
thing a DNA Calc host knows that Excel does not tell is the order in which it evaluated, and 4.7
types that apart. Two consequences. The owner's rule holds by construction. And a host that
answers these queries from a running Excel is possible later (the owner's note of 2026-11-12 on
linked boxes: "so the same view can run as an Excel add-in"); nothing in the contract would
change for it.

**Watches.** Any level-1 query can be watched. There are three delivery classes, and a view does
not need to know which it has:

- *Streams*: `Cells` over areas. The existing tile publication (stamp, fragments, barrier, one
  flight per subscription, coalescing into a reset under pressure) is kept as it is.
- *Tracked answers*: everything else. The host records each watched query's footprint (the
  sheets, areas and sections it read). A commit or a publication that touches the footprint
  marks the watch stale and re-runs it within a budget. Today the page re-asks every projection
  whenever the revision advances; the footprint narrows that.
- *Event lists*: calculation events and source updates, as they happen, coalesced.

```rust
pub enum Update<A> {
    Fresh(Answer<A>),
    /// The last answer no longer holds; a new one is coming. Views show this honestly.
    Outdated { since: Stamp },
    Refused(Refusal),
}
```

**Frames.** Everything the host has to say after one step (a commit, a publication, a source
update) reaches the page as one frame, and the runtime applies a frame whole before any view is
told:

```rust
pub struct Frame { pub stamp: Stamp, pub events: Vec<Event>, pub updates: Vec<WatchUpdate> }
```

The 17-harvest found one causality gap of this kind (finding 4): the formula bar read one mirror
while the grid read another that completed in a separate message. It was closed for that pair
under dc-1nbp.21. With several watches in every view, each view would have to solve it again;
a frame solves it once. A watch that is not ready when its frame goes is reported `Outdated`
in that frame. A macro that sets
`ScreenUpdating = False` holds frames back until it sets it again or ends; the Strip's "Macro
running" readout is not held.

### 4.7 Calculation state and evidence

Three questions are kept apart, with three answer types, because the designs' strongest push-back
was against blurring them (Pro push-back 2, Pro R-10):

| Question | Query | The answer says |
|---|---|---|
| What changed? | `Changes { generation }` | Values before and after. Nothing about order. |
| What could affect what? | `Deps`, `Formula` | References in the formulas. Not what ran. |
| What actually ran? | watch `CalcEvents` | The host's own record: admitted, reevaluated in this order, yielded, published, completed. Offered only when the host recorded it. |

```rust
pub enum Evidence<T> {
    Observed { by: Observer, at: Stamp, body: T },
    Derived  { from: Derivation, at: Stamp, body: T },    // a dependency explanation, a reading
    Unavailable { why: String },
}
```

- `CalcEvents` answers `Evidence::Observed`, or `Unavailable` on a host that kept no record.
  `Deps`, `Issues`, `Contributions` and the inferred items of `Structure` answer
  `Evidence::Derived`. `Changes` and everything read from the file are plain answers.
- The calculation events are today's `CalculationEvent` (`projection.rs:2236`, 12 kinds),
  unchanged. A view that shows them says whose order it is: DNA Calc's, not Excel's (Pro 21,
  I01 to I03).
- A circular reference under iteration is reported as a cycle with its members and settings, and
  how the iteration ended. It is never reported as a delay. A delay is a reference to an earlier
  period's cell, which `Deps` shows as an ordinary edge.
- A ticked model ticks from a source: the RTD clock (dc-hul4), or a macro that changes values and
  calls Calculate. Either way the page sees a source event and a new calculation generation. The
  contract has no simulation step of its own (D3).
- Asking never disturbs. A query, a preview and a print read a consistent snapshot and never
  consume a live observation or advance a clock (Pro 21, O03).
- Parity evidence is a level-0 fact: the catalogue entry of a function carries its recorded match
  status against Excel, from the witness bank, as the function browser of the Formula Explorer
  shows it.

### 4.8 Events

```rust
pub enum Event {
    Workbook(WorkbookEvent),     // opened, replaced, saved, save failed, state word changed, closed
    Committed(HistoryEntry),     // also for undo, redo and revert
    Calc(CalcProgress),          // started, progress, published, finished, cancelled: what the
                                 // Strip shows. The detailed record is the `CalcEvents` watch
    Selection { selection: Selection, cause: SelectionCause },   // an echo of a sequence, a macro,
                                 // an agent, an undo (4.2)
    Source(SourceEvent),         // link state, RTD update, provider loaded, failed or stopped
    Macro(MacroEvent),           // started, finished with what it changed, status text, a dialog to
                                 // answer, a view request (13)
    Proposal(ProposalEvent),
    Notice(Notice),              // typed and non-modal: a repair on open, a refused feature
}
```

Opening never raises a dialog. What Excel would ask about (links, macros, loops, repairs) arrives
as notices and as fields of `Summary`.

### 4.9 Ports, participants and carriers

```rust
/// Level 0. A view holding only this cannot ask about a workbook: the bound is the proof.
#[derive(Clone)]
pub struct FormulaPort { /* carrier handle */ }

impl FormulaPort {
    pub fn ask<Q: FormulaQuery>(&self, query: Q) -> Pending<Q::Answer>;
    pub fn run(&self, operation: FormulaOperation) -> Pending<Done>;
    pub fn events(&self, sink: impl FnMut(&FormulaEvent) + 'static) -> Listening;
    pub fn capabilities(&self) -> Capabilities;
}

pub enum FormulaOperation {
    LoadProvider(ProviderRef), UnloadProvider(ProviderRef), ReloadProvider(ProviderRef),
    StopProviderProcess, RestartProviderProcess,       // the surrogate, on the desktop
    LoadModule { name: String, source: String },       // a VBA module of functions
    SetMacroSecurity(MacroSecurity),
    SetEnvironment(Environment),                       // locale, date system
    OpenSession(FileRef), SaveSession(SaveRequest),    // the Explorer's own file
}

pub enum FormulaEvent {
    ProvidersChanged { functions: Vec<FunctionInfo> }, // the page's kernel answers from this list
    ProviderStopped { provider: ProviderRef, why: String },
    SourceUpdate { topic: String },                    // an RTD topic a trial reads has a new value
    Notice(Notice),
}

/// Level 1. Built on level 0, as the host is.
#[derive(Clone)]
pub struct WorkbookPort { /* carrier handle, the origin it stamps, its write policy */ }

impl WorkbookPort {
    pub fn formula(&self) -> &FormulaPort;
    pub fn ask<Q: WorkbookQuery>(&self, query: Q) -> Pending<Answer<Q::Answer>>;
    pub fn watch<Q: WorkbookQuery>(&self, query: Q,
                                   sink: impl FnMut(Update<Q::Answer>) + 'static) -> Watching;
    pub fn select(&self, selection: Selection) -> Sequence;
    pub fn navigate(&self, by: Navigation) -> Pending<Selection>;
    pub fn preview(&self, batch: &Batch, depth: PreviewDepth) -> Pending<Preview>;
    pub fn commit(&self, batch: Batch) -> Pending<Receipt>;
    pub fn propose(&self, proposal: Proposal) -> Pending<ProposalId>;
    pub fn run(&self, operation: Operation) -> Pending<Done>;
    pub fn frames(&self, sink: impl FnMut(&Frame) + 'static) -> Listening;
    pub fn policy(&self) -> WritePolicy;
}

pub enum WritePolicy { Commit, ApplyThenReview, Propose, Look }

/// What carries envelopes. Four implementations: web worker, WebView2, loopback socket, in process.
pub trait Carrier {
    fn send(&self, envelope: Envelope, bytes: Option<Vec<u8>>, answer: AnswerSink);
    fn listen(&self, deliveries: Rc<dyn Fn(Delivery)>, health: Rc<dyn Fn(Health)>);
}

pub enum Delivery { Frame(Frame), Formula(FormulaEvent) }
```

Who holds what:

| Participant | Port | Origin stamped | Reads | Writes |
|---|---|---|---|---|
| A person, through a view | `WorkbookPort`, handed to the view by the shell | `User`, with the view's id as a tag the page adds | Everything | `Commit`. The kit shows the seam where 4.4 says. |
| An agent | `WorkbookPort` from the agent bridge (10) | `Agent` | Everything, without blocking the user | `Propose` by default; `ApplyThenReview` or `Look` by the user's choice, per workbook |
| A macro, a COM client, an Office.js script | None: the host's own ingress, as today | `Macro`, `Automation` | Excel's object model | Excel's object model, grouped as a run |
| An installed component | The component port (9) | `Component`, through the view that hosts it | Its bindings | Intents on its own write bindings, which the kit turns into batches |

- `Pending<T>` resolves to `Result<T, Refusal>`. It is a single-threaded future with a `then`
  for callers that do not await. It names no toolkit.
- `Watching` and `Listening` end when dropped. A view that unmounts releases what it held
  without writing cleanup.
- `commit` on a port whose policy is `Propose` or `Look` is refused. Under `ApplyThenReview` it
  is accepted and its entry is marked unreviewed. The policy is fixed where the port is made,
  by the shell or the bridge, not by the participant.
- **Who is who is fixed by the connection.** The host gives each connection its origin (`User`
  for the page, `Agent` for the bridge) when it is made (5.3), and no message can change it.
  The page is one connection for all its views, so the view's id is a tag the page adds. Views
  are our own compiled code, and the tag is for the history's wording, not for trust.
- **One workbook, one port.** A port, a runtime and a history belong to one open workbook. The
  shell holds the list of open workbooks, and moving between them is navigation. What one
  workbook reads from another is Excel's external links: model content, reported by `Sources`.
- `Carrier` is today's `WorkbookTransport` (`dna-calc-model/src/transport.rs:9`: `start`,
  `set_events`, `set_health`, `call`) with the request and reply enums replaced by one envelope:
  an id, a kind (ask, watch, unwatch, select, preview, commit, propose, run, acknowledge), the
  query's name and its body.
- The page and the host are always built from one commit. The first message carries the
  contract's hash; a mismatch reloads. So the contract can change freely for as long as the
  build is one. The only code boundary that crosses versions at run time is the component
  port (9), and that one is versioned by hand. The stored formats (the view part, skins, pages,
  links) each carry a schema id and a version of their own.

### 4.10 Capabilities and refusals

```rust
pub enum Refusal {
    /// Not implemented: names what and the work package that owns it (honesty rule 2).
    NotModelled { what: String, owner: String },
    /// This host cannot do it; another can.
    NotAvailableHere { capability: Capability, route: Option<Route> },
    /// The base is behind, and these targets changed since: what the caller saw, what is there now.
    Conflict { conflicts: Vec<Conflict> },
    /// Excel would refuse this too, in Excel's words.
    Excel(ExcelRefusal),
    Protected(Target), Validation(ValidationFailure), ReadOnly,
    OverBudget { counts: Counts, ways_forward: Vec<WayForward> },
    NeedsProposal, Busy, Cancelled, Invalid(String),
}
```

`Capabilities` says what this host and platform can do: run VBA (and at which level), load .xll,
COM and .xlam add-ins, RTD, the file system, the system clipboard's formats, printing, whether
the file was opened read-only, and which key chords the platform keeps for itself. A view shows
a missing capability as a plain line with a route ("Open in the desktop app"), never as a
different calculation and never as an Excel error.

## 5. The hosts

### 5.1 Level 0: the formula host

`calc-formula-host`, portable. Its closure is calc-fml, calc-func, calc-catalogue, calc-value,
calc-vba for a loaded module, the provider interfaces, and calc-doc for its session file. Never
calc-engine.

```rust
pub struct Catalogue { pub search: String, pub family: Option<Family>, pub window: Window }
pub struct Analyse   { pub text: String, pub context: FormulaContext, pub caret: Option<u32> }
pub struct Present   { pub text: String, pub context: FormulaContext, pub level: LensLevel }
pub struct Interpret { pub typed: String, pub context: FormulaContext }
pub struct Evaluate  { pub trial: Trial, pub steps: bool }
pub struct Format    { pub value: CellValue, pub code: String, pub locale: Locale,
                       pub rules: Vec<ConditionalRule> }
pub struct Providers;

pub struct Trial {
    pub formula: String,
    /// The cell the formula would sit in. None in the standalone Explorer: a caller-dependent
    /// function is then refused with a route to a real cell, never given an invented caller.
    pub caller: Option<Caller>,
    /// A snapshot. Level 0 never reads a workbook; a missing input is reported, never defaulted.
    pub inputs: Vec<(Target, Shown<CellValue>)>,
    pub names: Vec<NameBinding>,
    pub environment: CalcEnvironment,        // date system, locale, clock
    pub base: Option<Stamp>,                 // set when level 1 made the trial
}
```

| Query | Answer | Runs |
|---|---|---|
| `Catalogue` | Functions: signature, argument help, examples, recorded match status against Excel | page and host |
| `Analyse` | Tokens with spans, references, diagnostics, completion at the caret, signature help | page and host |
| `Present` | The lens: the formula as shown at a lens level, with a map from every shown span back to its source span. Display only | page and host |
| `Interpret` | What a user typed in a view's vocabulary, turned into exact Excel text, or a refusal when a short name is ambiguous | page and host |
| `Format` | Shown text, colour and matched section; which conditional rules fired. The value and its text stay apart | page and host; rules that hold a formula, host |
| `Evaluate` | Value, type and shape; every sub-expression's value when asked; which inputs were read; which providers ran | host |
| `Providers` | Loaded add-ins, what each registered, where each can run | host |

```rust
pub struct FormulaHost { /* functions, RTD topics, environment, macro policy, the session file */ }

/// Host side: one impl for each query a host answers. `handle` finds it by the query's name.
pub trait Answers<Q: Query> {
    fn answer(&mut self, query: &Q) -> Result<Q::Answer, Refusal>;
}

impl FormulaHost {
    pub fn run(&mut self, operation: FormulaOperation) -> Result<Done, Refusal>;
    pub fn handle(&mut self, envelope: Envelope) -> Reply;        // from a carrier
    pub fn take_events(&mut self) -> Vec<FormulaEvent>;
    pub fn functions(&self) -> &CallableTable;                    // what level 1 calculates with
}
impl Answers<Evaluate> for FormulaHost { /* and one impl for each level-0 query */ }
```

- **What it is made from.** The core is the Formula Explorer's `Session`
  (`apps/dna-onecalc/src/session.rs`, 1,386 lines) with `cf.rs`, `file.rs`, `trace.rs`, `rtd.rs`,
  `fixture.rs` and `shown.rs`: 5,342 lines of pure logic that move as they are, with five small
  files beside them (598 lines). Its own port (`port.rs`, 32 requests and 17 replies) is
  replaced by the queries above. Entry reading (`interpret_entry`, `entry.rs:88`) moves here
  from calc-host: it is formula-level work that level 1 also needs.
- **RTD.** The admission rules are already beneath both hosts, in `calc_fml::rtd::admit`. What a
  level-0 session holds is the ledger of topics and the connector, as the Explorer's
  `SessionRtd` does. calc-host keeps its engine-side step (`recalc.rs:452-552`: an accepted
  update invalidates the topic in the engine) and takes the ledger and the connector from the
  level-0 session it owns.
- **What a level-0 session holds.** Registered functions (add-ins, a loaded VBA module), RTD
  topics, the environment (locale, date system) and the macro policy. In a workbook, level 1
  owns one such session for the workbook's providers, and `WorkbookPort::formula` hands out its
  port. A function registered there is the one the workbook calculates with. A view handles one
  session object either way.
- **The first decision the brief asked for.** Level 0 sits beside and below calc-host, as its own
  crate (the brief's option b). A crate boundary is checked by the dependency checker and holds.
  A cargo feature inside calc-host would unify across the workspace the first time any member
  enabled it.
- **Level 0 runs on both sides.** Parsing, completion, the lens and number formats are pure, so
  the page answers them itself with no round trip (the page calls calc-fml and calc-func today
  for exactly this). The pure answers live in one small crate, `calc-formula-kernel`, which the
  page's runtime and `calc-formula-host` both call, so there is one implementation. The page
  links the kernel and not the host, which keeps calc-doc out of the page (rule 5). The page
  answers from the function list the level-0 session publishes (`ProvidersChanged`: names,
  signatures, flags), so completion knows an add-in's functions without calling one. Anything
  that has to run a function goes to the host. A view cannot tell and does not need to.
- **The lens is `Present`, and it is display only.** It is the presenter of
  `LINKED_BOXES_SKIN.md`. `Interpret` is the interpreter beside it and is not the lens: it
  turns what a user typed in a view's vocabulary into exact Excel text, and the seam shows that
  text before anything is stored (Fable R-73). Both work on the syntax tree, never on text,
  and belong in calc-fml beside `assist`. "Rewrite as LET" is not a lens level: it is an
  `Enter` with a preview.
- **Add-ins stay where they run.** On the desktop the level-0 host loads .xll and .xlam through
  `calc-addin-host`, in process or in the surrogate process (`calc-surrogate-wire`, kept). In a
  browser `Providers` answers with what cannot run here and the route to a host that can.

### 5.2 Level 1: the workbook host

`calc-host` stays what it is: the one owner of the authored revision and the engine state. Seven
changes. Six add to its design, and the second replaces one part of it:

1. It takes level-0 services from `calc-formula-host`.
2. The view part replaces `SkinLayout`. `ModelRoot.layout` becomes `ModelRoot.view_part`:
   preferences, anchors, opaque documents, notes, trail. `SkinLayoutEdit` and
   `presentation_commands.rs` go; generic patches come.
3. Anchors follow structural edits. The transform calc-fml applies to references on an insert,
   delete, move or sort (`StructuralEdit`) is applied to anchors in the same transaction.
4. History entries gain a label, an origin, a kind, a group and a reversibility. Run groups have
   `begin_run` and `end_run` (17.1).
5. `prepare` is exposed without `commit`, for preview.
6. The root records its model revision apart from its view revision (4.3).
7. A batch with a base is checked for conflicts: what each later entry touched (areas, names,
   objects) is in the history, and the check intersects that with what the batch writes and
   expects.

```rust
impl HostSession {
    pub fn formula(&mut self) -> &mut FormulaHost;                                // 1
    pub fn prepare(&mut self, changes: &[HostChange]) -> Result<Prepared, IntentError>;   // 5: no commit
    pub fn begin_run(&mut self, procedure: &str) -> RunId;                        // 4
    pub fn end_run(&mut self, run: RunId, outcome: RunOutcome);
    pub fn checkpoint(&self) -> Checkpoint;                                       // a pinned root
    pub fn restore(&mut self, checkpoint: &Checkpoint) -> IntentReceipt;
    pub fn revisions(&self) -> (AuthoredRevision, AuthoredRevision);              // 6: model, view
    pub fn conflicts(&self, base: AuthoredRevision, on: &[HostTarget]) -> Vec<HostConflict>;   // 7
}
```

`HostChange` is the host's form of a change: one of today's intents, or a patch of the view
part. A `Prepared` can be committed or dropped. A `Checkpoint` is a pinned root. A `HostTarget`
is cells, a name, a table or an object, because a batch can expect any of them. `prepare` takes
the session mutably because the engine reserves node ids while it prepares
(`calc-engine/src/edit.rs`), and gives them back when the preparation is dropped.

The serving crate reaches everything else through `dispatch`, `project`, `subscribe` and the
other calls the worker uses today. The new queries of 4.6 add read-only functions beside them.

### 5.3 The serving crate

`calc-skin-host` turns the contract into calls on the two hosts. It is what
`apps/dna-calc-worker/src/server.rs` is today, without the per-view projections.

```rust
pub struct SkinHost { /* sessions, connections, watches, publications, proposals */ }

impl SkinHost {
    pub fn connect(&mut self, origin: Origin, policy: WritePolicy) -> ConnectionId;
    pub fn handle(&mut self, from: ConnectionId, envelope: Envelope, bytes: Option<Vec<u8>>) -> Reply;
    pub fn advance(&mut self, budget: WorkBudget) -> HostProgress;   // calculation and index work
    pub fn take_frames(&mut self, to: ConnectionId) -> Vec<Frame>;
}
```

- One session can have several connections: the page, and an agent's bridge. Each has its own
  origin, policy and watches. An agent's reads are served between the page's frames, within a
  budget, so reading never blocks the user (Fable C-49).
- Kept from the worker: `publication.rs` (flow control), `workbook.rs` and `cells.rs` (one cell
  assembly), `format.rs`, `autofit.rs`, `search.rs`, `names.rs`, `printing.rs`, `drawn.rs`,
  `picture.rs`, `omml.rs`, `parts.rs`, `custom_geometry.rs`, `model_clock.rs`: 6,921 lines. From
  `server.rs`, the open and save flows, paste planning, validation decisions, name calculation
  and the driving of publications are kept and re-keyed to the envelope.
- Deleted: `canvas.rs`, `ledger.rs`, `tree.rs`, and the `Notebook`, `BlockDetail`, `Canvas`,
  `Ledger`, `Atlas`, `AtlasRefusals`, `Outline` and `Dependents` handlers of `server.rs`. The
  wire drawings the worker makes for the canvas skin go with them.
- **Charts stay drawn by the host.** A workbook chart is content, drawn as Excel draws it, not a
  view's presentation. One renderer (`calc-chart-render`) serves the pane, `Chart.Export` and
  print, and it depends on calc-host today. `Objects` returns the drawing and the series data
  beside it, so "Open chart data" and a screen reader's table come from the same answer. A view
  that wants a different picture of the numbers (an instrument's plot) draws its own from data
  with the kit. This differs from the implementation brief's item 3, which would move chart
  drawing to the page; section 18 gives the reason.
- From the 17-harvest: the amplification findings (conditional-format resets, the staging copy,
  the recounts) were closed under dc-1nbp.20 and reply visibility under dc-1nbp.21, and those
  fixes are kept. Still to do in the rebuild: the page routes frames by the stamp's workbook as
  well as by subscription id (the harvest's optional guard), and the desktop carrier stops
  sending bytes as base64 (17.2).

## 6. The page: runtime, kit and shell

### 6.1 The runtime

`dna-skin-runtime` is portable and names no toolkit. It is what `apps/dna-calc-model` is today
without the wire types, which move to `calc-skin`. Its logic is tested natively and in both
browsers, as the model crate's is now.

```rust
pub struct Runtime { /* one per open workbook, or per formula session at level 0 */ }

impl Runtime {
    pub fn formula(&self) -> &FormulaPort;
    pub fn workbook(&self) -> Option<&WorkbookPort>;          // none at level 0
    pub fn stamp(&self) -> Live<Stamp>;
    pub fn selection(&self) -> Live<Selection>;
    pub fn mode(&self) -> Live<Mode>;                         // Ready, Enter, Edit, Point
    pub fn edit(&self) -> &EditSession;                       // the one draft: target, text, caret, picks
    pub fn input(&self) -> &InputQueue;
    pub fn commands(&self) -> &CommandTable;
    pub fn keymap(&self) -> &Keymap;
    pub fn places(&self) -> &Places;                          // Back and Forward
    pub fn document(&self, doc: DocId) -> DocHandle;          // a view document, edited through batches
    pub fn local<T: LocalState>(&self, key: LocalKey) -> Local<T>;   // per user, not in the file
}

pub enum Mode { Ready, Enter, Edit, Point }
pub struct ModeFlags { pub end: bool, pub extend: bool, pub add: bool, pub scroll_lock: bool }
```

`Live<T>` is a value with `get` and `subscribe`. The kit turns one into a signal of its toolkit.

What the runtime owns:

- **The store.** The last answer of every watch, and the tile mirror for `Cells` streams
  (`tile_mirror.rs`, kept). It applies a frame whole, then tells the views.
- **The edit session and the modes.** One draft per workbook, whichever view started it. It
  survives a view switch, because it is not the view's (Pro R-4). Excel's four modes and its End,
  Extend and Add states become real states here; today there is one flag.
- **The input queue.** Input is ordered. A key the runtime can resolve from what it holds (a move
  inside the mirrored area, a character into the draft) is applied at once. A key that needs the
  host (Ctrl+arrow to a data edge outside the mirror, a commit) is sent, and every later key
  waits behind it and is replayed in order when the answer comes. Type-ahead is exact: the result
  is the same as if each key had waited. The page does this today in a capture listener
  (`input_adapter.rs:615-710`); the rule moves into the runtime, with a thin adapter in the kit.
- **The command table and the keymap** (8.3, 8.5). Input goes to the focused editor or control,
  then the view's layer, then the shell's (Pro R-16).
- **Places.** Back and Forward over selections, views, sheets and panels. Not the history.
- **Local state.** Scroll, camera, panel sizes, drafts kept for recovery, recordings: per user,
  per workbook, never in the file (12.6).

### 6.2 What a view implements

The trait lives in `dna-kit`, so the contract and the runtime stay free of the toolkit.

```rust
pub trait View: 'static {
    fn spec(&self) -> ViewSpec;
    /// Before mounting: can this view show this scope whole, simplified, or not at all?
    fn fit(&self, counts: &Counts, part: &PartSummary) -> Fit;
    fn mount(&self, cx: ViewCx) -> Mounted;
}

pub struct ViewSpec {
    pub id: ViewId,                          // "classic", "inspector", "notebook", "map", "app"
    pub title: &'static str,
    pub scopes: &'static [DocScopeKind],     // a sheet, the workbook
    pub settings: SettingsSchema,            // what a skin may set for this view
    pub document: Option<DocSchema>,         // what it keeps in the view part
    pub commands: &'static [CommandSpec],
    pub keymap: &'static [Binding],          // the view's own layer
}

pub enum Fit {
    Whole,
    Simplified { shown: u64, of: u64, how: String },
    Declined { size: Counts, why: String, ways_forward: Vec<WayForward> },   // at least two
}

pub struct ViewCx {
    pub rt: Runtime,
    pub scope: DocScope,
    pub document: Option<DocHandle>,
    pub settings: Settings,                  // from the active skin
}

pub struct Mounted {
    pub body: AnyView,
    pub locate: Box<dyn Fn(&Selection) -> Located>,
    pub reveal: Box<dyn Fn(&Selection)>,
    pub print: Option<Box<dyn Fn(&PrintRequest) -> PrintJob>>,
}

pub enum Located { Exact, Within { item: ItemRef, label: String }, Absent }
```

- A view is given a runtime and nothing else. It cannot find the shell, another view or a
  carrier (rule 2).
- `fit` is asked before `mount`, from the counts `Census` answers, which need no calculation.
  A view that declines never mounts and never loads what it cannot show. The shell draws the
  declining state, so every view declines in the same words (Fable R-80).
- **Panels and components are registered as views are.** A crate can export panels (an id, a
  title and a mount over the same context) and components (an id and a binding contract). The
  composition registers them. The Inspector's instruments reach every view as panels this way,
  and the grid reaches pages as the `sheet` component, without the shell or the kit naming
  either crate.
- A level-0 tool implements the twin, `FormulaView`, whose context holds a `FormulaPort`, an
  optional trial handed in by the shell, and an optional `apply` callback. The Explorer never
  names a workbook port, inside DNA Calc or outside it (7.7).
- Today's `Skin` trait (`apps/dna-calc/src/skin.rs:59`) has the same job with `id`, `title`,
  `mount` and three focus hints. The registry idea and "a view that cannot be shown says so" are
  kept. The `Controller` it hands out is what goes.

### 6.3 The kit

`dna-kit` holds the basic components every view and every page is built from, the page
renderer, and the registry that components are looked up in by id. Each component has a typed
binding contract, draws provenance and coverage itself, carries the test roles
(`data-dnacalc`) and the accessibility roles, and windows what is large.

| Component | Binds to | Shows | Writes |
|---|---|---|---|
| `Value` | a cell, a name | The value in its own format, its provenance mark, its source on request | none |
| `Input` | one cell | A field, slider, stepper, dropdown, checkbox or date, from the cell's validation and the control declared | `Enter`, under validation and protection |
| `DataFrame` | a table, a spill, a range | Windowed rows, "n of N", view sort and view filter | optionally `Enter` at the source row |
| `Chart` | a chart object | The host's drawing of the Excel chart; "Open chart data" | none |
| `Plot` | a recording, a range | A line or bar plot the kit draws itself (instruments) | none |
| `Text` | markdown with quotes | Prose with live quoted values | none |
| `Formula` | a formula cell | The formula with spans, lens levels and chips; edits through the shared field | `Enter` |
| `Control` | an Excel form control | The control; a click writes its linked cell as Excel's does | `Enter` on the linked cell |
| `Scenario` | the workbook's scenarios | Pick, compare, save | `Scenario` changes |
| `Button` | a command, or a macro | Runs it under the rules the Deck's button has | by its command |
| `Peek`, `Seam` | any target; a preview | The peek card; the seam in its two forms | commit or cancel |

Registered from other crates, and used through the same registry:

| Component | From | Shows |
|---|---|---|
| `sheet` | `dna-grid` | The grid over a range: a bounded excerpt with exact addresses, or the whole stage |
| `trace`, `links`, `issues`, `sources`, `watch`, `changes`, `notes` | `dna-view-inspector` | The instruments (7.2) |
| any installed component | an extension (9) | What its package declares |

The kit also holds the adapters to the browser (input, clipboard, focus, measuring text) and
uses `dna-formula-ui`'s formula field, which every editor in every view shares. The grid is
not in the kit: `dna-grid` is a crate of its own, so that the kit stays a set of small parts
and lanes working on views do not all meet in one crate.

### 6.4 The shell

`dna-shell` is everything every view shares, in the same places in every view (Fable R-1):

- **Mast**: the file and its state word, the views, one field for commands, addresses and
  requests to the agent (Alt+Q), the agent's chip.
- **Deck**: one row of command bands. Tab names and key-tip letters are Excel's; the look is
  ours. It does not exist today.
- **Bridge**: the address field and the formula bar, in every view.
- **Stage**: the view, and at most one companion beside it (the grid by default), sharing the
  selection. Never more than two panes.
- **Panels**: docked right or below. The instruments, and any page a skin declares.
- **Sheet tabs**, with each sheet's preferred-view badge.
- **Strip**: mode word, calculation, errors, outside data, macros, agent, statistics. Never
  hidden, under any skin.
- Over the top: the seam, trust cards, the try-on band, notices, the dialogs Excel users expect
  (Format Cells, Paste Special, Name Manager, Page Setup, Find and Replace), file and print.

The shell is the only place that asks `Summary`, runs file operations, mounts views from the
registry and shows what a macro or an agent asks for. At level 0 it shows Mast, stage and Strip
only, from skin 0's declaration: the Formula Explorer has the same shell, not a second one.

## 7. The views and the tools on the contract

For each: what it reads, what it keeps in the view part, what it changes, and what it is built
from. None of them has a query of its own.

### 7.1 Classic

- **Is:** the grid, with Excel's keyboard and mouse, under the shared shell.
- **Reads:** the `Cells` stream over one area per pane (the union subscription the grid has
  today), `Geometry`, `Objects`, `Rules` at the active cell, `Stats`.
- **Keeps:** nothing in the view part. Panes, zoom, the active cell and each sheet's selection
  are Excel's own sheet-view settings. They are model content, as today.
- **Changes:** the whole model vocabulary. No presentation change of its own; an overlay
  (provenance tints, block outlines) is a setting of the skin.
- **Built from:** `dna-grid`, the grid as a component: the render plan, geometry, key grammar
  and viewport of the model crate; the pure parts of `apps/dna-calc/src/grid/` (panes, axis,
  decoration, alignment, the two painters); and the gesture code of `grid/mod.rs` (4,296 lines
  of Leptos state) written again over the runtime's modes and input queue. Classic is that
  component given the whole stage. The same component is the companion pane beside any view,
  the sheet page inside the map and the `sheet` part of a page, so the grid behaves the same
  wherever it appears.
- **New:** the Deck and its key tips; Excel's modes as states; the keymap as a data table (8.3).
- **Fit:** Classic never declines a workbook.

### 7.2 Inspector and the instruments

An instrument is a component over one or two queries. `dna-view-inspector` exports each one as
a panel and the composition registers them (6.2), so every instrument docks in every view (the
README's agreement, from Fable's and Carbon's first push-backs). The Inspector view is the
same instruments given the stage, arranged by a page, so a skin can rearrange the bench.

| Instrument | Reads |
|---|---|
| Details (the peek card, pinned) | `Cells`, `Formula`, `Rules` and `Structure` at the selection |
| Trace | `Explain`, `Contributions` |
| Precedents, Dependents | `Deps` |
| Issues | `Issues` |
| Calculation | `CalcState`, a watch on `CalcEvents`, `Changes` |
| Live | `Sources`, with recordings |
| Watch | `Cells` watches on pinned targets |
| Overview | `Inventory`, `Census`, `Structure` |
| Changes | `History`, `Proposals` |
| Notes | `ViewPart { notes }`: the notes and findings at the selection, and all of them |

- **Keeps:** the pinned signals and the bench page, in its view document.
- **Recordings** are the runtime's, per user. Record, Pause capture and Clear act on the
  recording and never on the source (Pro 21, I01). A replay writes nothing.
- **Changes:** a note or a finding is a presentation change (`Note`), written to the view part
  (D5). A repair it offers is a model batch through the seam.
- **Honest by type:** what an instrument may claim comes from `Evidence` and `Coverage`. A trace
  with no recorded order says so; a contribution count over part of the rows says which part.
- **Fit:** it starts from the selection and one frontier. A whole workbook starts with counts.

### 7.3 Notebook

The notebook works exactly like Jupyter (D6). The chain is the thing: an input, its result
beneath it, the next input using it.

| Jupyter | Here |
|---|---|
| A notebook: an ordered list of cells | A notebook document in the view part, scoped to one sheet, its backing sheet |
| A code cell: source, and its output beneath | One Excel formula as written, and beneath it the value of its backing cell or spill |
| A markdown cell | Prose kept in the view part, quoting cells live |
| Rich output: a table, a plot | The kit draws the output by its type: `Value`, `DataFrame` for an array or a table, `Chart` for a chart object placed as a cell |
| A name defined in one cell and used in a later one | A defined name or an address. A later formula refers to an earlier output as any Excel formula does |
| The kernel, busy or idle | The workbook's calculation, as the Strip shows it |
| Run cell: Shift+Enter, Ctrl+Enter, Alt+Enter | Commit the formula to its backing cell. Under automatic calculation everything that depends on it follows at once. Under manual calculation only what was run is fresh, and the rest is marked stale until F9 |
| Run all; interrupt | Calculate now; cancel the calculation |
| Command mode and edit mode: Enter, Esc, A, B, D D, M, Y, J, K | The same keys, as the Notebook's own keymap layer. Inside a cell's editor the keys are Excel's formula keys: F2, F4, F9, pointing |
| Save | Save the workbook: formulas in cells, order and prose in the view part |
| A read-only notebook | A capability of how the file was opened. The same presentation, with editors off |

```json
{ "schema": "dna.notebook/1", "layout": "down",
  "cells": [
    { "id": "c1", "kind": "markdown", "text": "## Paid orders\nTotal {{Analysis!J2}}" },
    { "id": "c2", "kind": "code",     "anchor": "a17" },
    { "id": "c3", "kind": "object",   "anchor": "a21" },
    { "id": "c4", "kind": "page",     "page": { "…": "a control panel, in the page grammar (8.4)" } }
  ] }
```

Where this is more than Jupyter, it follows the owner's list of 2026-11-20: under automatic
calculation results follow their inputs at once, as in Pluto and Excel; the chain can be laid
out across as well as down; and a workbook that has no notebook yet can be read as one, from a
draft. Where it is not Jupyter's: there is no kernel state apart from the workbook, and what
deleting a cell does to a cell placed from an existing sheet is an open point (section 19,
point 4).

- **Reads:** its document, `Cells` watches on each cell's anchor, `Formula` for the lens and the
  LET tree, `Rows` for data frames, `Objects` for charts, `Deps` to mark what a draft makes
  stale.
- **Running a cell** shows the seam's label (the backing cell it writes) and commits on
  Shift+Enter. Nothing waits for an Apply.
- **Creating a code cell is one batch:** `Enter` into a backing cell that `Placement` finds
  (spills get their own column lanes, so growth downwards does not collide: Pro 21, NC06), a
  `Name` when the author names the output, and the `Doc` patch that places the cell. The seam
  shows the place and lets it be changed.
- **Deleting a cell** removes it from the document, and for a cell the notebook created it also
  clears the backing cell, in the same batch. The seam lists what would then show an error
  (section 19, point 4, leaves the default for placed cells to the merge).
- **A draft that does not parse** stays in the edit session. What depends on it is marked
  "previous result" from `Deps`, and Esc restores the committed formula (Pro 21, NC02).
- **A data frame's sort and filter** are the component's own and change nothing. "Change the
  table" is a separate model change through the seam's review (Pro 21, N01).
- **A workbook with no notebook document** still opens here. The view drafts one from
  `Structure` (a run is one cell, a table is a data frame) and says it is a draft. Nothing is written
  until the author keeps it, and keeping it is a presentation change.
- **Layout** down or across is a setting. Calculation order does not follow it.
- **Fit:** it declines a whole dense workbook and offers one sheet, the outputs the user picks
  with what feeds them, or an outline.

### 7.4 Model map

- **Reads:** `Structure { level }` for sheets, blocks, names and tables as boxes; `Deps` grouped
  for wires and bundles, with counts and kinds; `CalcState` for cycles; `Sources` for clocks and
  feeds; `Cells` watches for the values on ports; `Changes` for the ripple.
- **Keeps:** positions, folds and the user's own groups, by anchor. The camera is local.
- **A reader first.** Then edits on named ports only: `ReplaceReference` moves an existing
  reference, `RenameName` renames a box, `Enter` edits a box's formula. Making a box writes a
  label cell, a value cell and a `Name` on a visible sheet, at a `Placement`, always through the
  seam. A wire with nowhere to go in the formula is refused (4.4).
- **"x of y accounted for"** is read off the coverage and counts of the `Deps` answer, so it is
  exact or says it is partial (Fable R-48).
- **The ripple** is drawn from `Changes` and labelled a dependency explanation, unless the host
  recorded an order (4.7).
- **Cycles** come from `CalcState` and are drawn as an iteration enclosure with their settings. A
  delay mark is drawn only where a reading says the reference is to an earlier period. Without
  evidence the relation is labelled Unknown (Pro 21, M02).
- **A ticked model's Run, Pause, Step and Rate** are inputs of its clock formula
  (`=RTD("DnaCalc.Clock",,model,"tick",rate,running,step)`). The controls are `Input`s on those
  cells: ordinary model changes, the same workbook in Excel with the clock add-in.
- **Fit:** 200 boxes and 400 wires in a viewport. Above that it shows counted groups and a
  focused submap, and refuses "expand all" with ways forward (Pro 21, M03).

### 7.5 App

- **Reads:** only what its pages bind: `Cells` watches on bound areas, `Rules` for each input,
  `Objects` for charts and controls, the workbook's scenarios.
- **Keeps:** its pages (8.4), the input mode (live, or batched with the author's reason), the tab
  order and the print layout.
- **Changes:** `Enter` on its inputs, under the workbook's own validation and protection, and
  scenario changes. A live input shows the seam's label on focus and commits on Enter. Batched
  mode holds drafts in the runtime and commits them as one batch at Apply.
- **No page declared:** it says so and offers Classic and the Forms maker. A broken binding is
  shown as broken. It never falls back to a nearby cell.
- **Cannot:** hide the Strip or the way to Classic, or run a macro the security level refuses.

### 7.6 Forms maker

The easy way to make a page, and the place D6 puts the reader-style presentation.

- **Is:** a second `View` in the `dna-view-app` crate, which the shell mounts beside the App
  stage: candidates on one side, the page itself live in the middle, properties on the other.
- **Candidates** come from `Structure` and `Rules`: inputs from unlocked cells, validated cells
  and cells linked to controls; outputs from cells nothing reads, charts and tables. Certain
  evidence only (Fable R-55). The author adds the rest by pointing.
- **Everything it does is a presentation change.** A page lives where its owner is. An App page
  or a notebook's control panel is in a view document, and editing it is a `Doc` patch. A
  panel is in a skin, and editing it changes that skin (8.6). The page on screen is the
  try-on. Keep commits it.
- **When the author wants something of the model** (a validation rule, a name, protection), the
  Forms maker offers it as a separate model batch through the seam. Looks and model never mix.
- **A page with no inputs is a report**: text, values, frames and charts in reading order. That
  is the reader-style presentation; the Notebook has none of its own.
- **The same pages** dock as panels and sit inside notebooks as control panels, so the Forms
  maker edits those too. An agent makes the same page by writing it as JSON (10.2).

### 7.7 Formula Explorer (skin 0)

- **Is:** a `FormulaView` on level 0. Its facets: the formula and its inputs with a try column
  (`Evaluate`), steps (`Evaluate` with steps), number format and conditional format (`Format`),
  the function browser with each function's recorded match against Excel (`Catalogue`), add-ins
  (`Providers`), and saved tests with expected and observed results.
- **Inside DNA Calc:** opened on a cell, the shell asks level 1 for `TrialFor { at }` and hands
  the trial in. The Explorer overrides inputs in the trial, never in the workbook. "Apply to
  cell" calls back into the shell, which builds a batch with the trial's base and the cell under
  `expects`, and shows the seam. A base that is behind is refused with the conflict (Pro R-38).
- **Standalone:** the composition `dna-onecalc` is the shell, this view, skin 0 and the level-0
  host. There is no workbook and no caller. A caller-dependent function is refused with a route
  to a real cell.
- **A provider that is unknown or has side effects** is not run on every keystroke. `Providers`
  says which can preview safely; the rest wait for Evaluate (Pro R-42).
- **On the site:** the same build, embedded in a page, opened by a link (14).

### 7.8 In one table

| | Reads | Keeps in the view part | Model changes | Host work it waits for | A first working version |
|---|---|---|---|---|---|
| Classic | `Cells` stream, `Geometry`, `Objects`, `Rules` | nothing | all | none | weeks: the conformance tail (17.3) |
| Inspector | `Deps`, `Explain`, `Issues`, `Sources`, `CalcState`, `CalcEvents`, `Changes` | pinned signals, bench page | repairs through the seam | `Deps` grouped, `Issues` by root, `Sources`, `Contributions` | hours for each instrument once its query answers |
| Notebook | its document, `Cells`, `Formula`, `Rows`, `Objects`, `Deps` | cells in order, prose | `Enter`, `Name` | `Placement`, `Rows`; `Structure` for the draft | hours for the chain; the draft follows |
| Model map | `Structure`, `Deps`, `CalcState`, `Sources`, `Changes` | positions, folds, groups | later, on named ports | `Structure` levels, `Deps` grouped | hours as a reader |
| App | bound `Cells`, `Rules`, `Objects` | pages | `Enter`, `Scenario` | none | hours |
| Forms maker | `Structure`, `Rules` | edits pages | offers, through the seam | `Structure` for candidates | hours after App |
| Explorer | level 0 only | nothing (its session file) | none of its own | none | hours: the logic exists |

"Hours" is the owner's expectation in D10 and this design's aim, not a measurement. It is for one
lane, with the kit's components in place. The host work in the fifth column is separate and is
not hours (16).

## 8. Skins: the manifest, tokens, keymaps, pages and commands

### 8.1 The manifest

A skin is one JSON document. It has no code, no expression language and no address to fetch.

```json
{
  "schema": "dna.skin/1",
  "id": "calm-review",
  "name": "Calm review",
  "extends": "dna:default",
  "theme":    { "base": "cockpit-light", "density": "working",
                "tokens": { "interface.scale": "110%", "formula.size": "15.5px" } },
  "shell":    { "deck": "compact",
                "panels": { "right": ["trace", "watch"] },
                "strip": [ { "value": "Summary!B7", "label": "Profit" } ] },
  "keymap":   { "profile": "excel-windows",
                "bindings": [ { "when": "view:classic mode:ready", "chord": "Alt+Shift+X",
                                "run": "audit.next-exception" } ] },
  "commands": [ { "id": "skin.total-border", "label": "Total border",
                  "run": [ ["format.borders", { "top": "thin", "bottom": "double" }],
                           ["format.bold", { "on": true }] ] } ],
  "views":    { "classic":  { "overlay": { "provenance": true }, "gridlines": false },
                "notebook": { "layout": "down" } },
  "pages":    { "audit-rail": { "…": "a page, 8.4" } },
  "requires": { "components": [ { "id": "acme.yield-curve", "version": "^1",
                                  "fallback": "data-frame" } ] }
}
```

A page in a skin is a panel or a bench that the skin's owner arranged. A page that belongs to a
workbook (an App page, a notebook's control panel) lives in that workbook's view document, not
in a skin.

```rust
pub struct Skin {
    pub schema: SchemaId,                  // "dna.skin/1"
    pub id: SkinId,
    pub name: String,
    pub extends: Option<SkinRef>,          // what is not given is the parent's
    pub level: Level,
    pub theme: ThemeChoice,                // a base theme, a density, token values
    pub shell: ShellLayout,                // Deck contents, panels, Strip extras, quick slots
    pub keymap: KeymapChoice,              // a profile and added bindings
    pub commands: Vec<NamedCommand>,       // fixed sequences of existing commands
    pub views: BTreeMap<ViewId, Value>,    // each checked against that view's SettingsSchema
    pub pages: BTreeMap<PageId, Page>,
    pub requires: Requirements,            // installed components, by id and version
}
```

Where a skin lives, and the order in which they apply:

| Where | Travels | May set | Applies |
|---|---|---|---|
| Built in: `dna:default`, and `dna:explorer`, which is skin 0 | with the app | everything | always, underneath |
| The user's | stays with the user | everything | to every workbook |
| The workbook's, in the view part | with the file | everything | Its view settings and pages apply on open: they are the file's presentation. Its theme, keymap, shell layout and named commands are offered under the try-on band and never applied unasked |
| A try-on | nowhere | everything | until Keep or Discard |

A file can make a workbook open as its App. It cannot rebind a stranger's keys or restyle their
application without being asked.

### 8.2 Tokens, and what stays fixed

The tokens are Strand's, from `dna-strand`: three base themes (`cockpit-light`, `cockpit-dark`,
`high-contrast`), two densities, 23 colour tokens per theme, the radii (5, 9, 14), the gap scale
(2, 4, 6, 10, 14, 22) and the settle time (120 to 160 ms), emitted as `--dna-*` properties. A
skin picks a base theme and a density and may give values for the tokens that are open to it.

| Open to a skin | Closed to a skin |
|---|---|
| Surfaces, neutral inks, lines, chrome and the accent; type sizes; the interface scale; a view's own settings (gridlines, overlays) | The signal colours with their inks and tints (amber, red, orange, green, and the provenance inks); the two strokes of the focus ring; the radii and the gap scale |

The check that runs before a skin is applied is the test Strand already has, made callable: the
61 sanctioned pairs (`contrast.rs:77`) are computed again with the skin's values, and a pair that
falls below its ratio refuses the skin and names the pair and the ratio.

Fixed, whatever the skin:

- the workbook's own formats, which are the author's and are drawn as Excel draws them;
- the five signals and their shapes (amber is now, red is an error, orange is outside or volatile
  data, green is fresh, teal is structure): set by the base theme only;
- the line-style channel (solid is stated, dashed is inferred, dotted is a ghost);
- the Strip's readouts, the seam, Esc as an exact revert, the address grammar, trust cards and
  the way to Classic;
- every permission: macros, add-ins, components, sending anything outside.

### 8.3 Keymaps and key tips

```rust
pub struct Binding { pub when: When, pub chord: Chord, pub run: CommandId, pub args: Value }
pub struct Chord   { pub key: Key, pub ctrl: bool, pub shift: bool, pub alt: bool, pub meta: bool }
pub struct KeyTip  { pub path: &'static str, pub run: CommandId }     // "H,B,B", after Alt
```

- **The Excel profile is a table of data**, `excel-windows`: one row per key or mouse gesture of
  the conformance list, with its context (mode, kind of selection), the command, the expected
  result, the tier and the Excel build it was observed on. The Classic view's keymap layer is
  read from it, and so is the scripted test for each row. One table, so the keymap and its test
  cannot drift. Today the code holds 38 sheet bindings and 13 shell bindings
  (`selection.rs:619`, `command.rs:335`).
- **Layers**, first match wins: the fixed keys (Esc, Undo and Redo, the palette, the way to
  Classic); the focused editor or control; the skin's bindings; the view's layer (Classic's is
  the profile, the Notebook's is Jupyter's); the shell's.
- **DNA Calc's own keys are few and in the shell's layer:** Alt+Q for the palette (Excel's own
  search key), Ctrl+Alt+1…5 for the views (D12), F6 for regions. D12's condition is a test: no
  chord of the shell's layer is bound by the profile table in a context they share.
- **Key tips are a path table**, Excel's letters exactly (D11), apart from the Deck's look. Alt,
  H, B, B runs "bottom border" whatever a skin puts on the Deck (Fable R-23). The table is
  captured from Excel on Windows.
- **A skin may add bindings freely.** Changing a key of the profile is allowed and counted: "this
  skin departs from Excel in 2 keys" is computed from the tables (Fable R-94). The count
  includes the Alt paths: a skin that binds a chord Excel uses to open a tab's key tips departs
  by that key. The fixed keys cannot be rebound.
- **What the platform keeps** (a browser's Ctrl+W, Ctrl+T, F11) comes from `Capabilities`. The
  shell shows the alternate, and the page "keys that differ here" is generated from the same
  data.

### 8.4 Pages

One grammar for everything laid out by declaration: an App page, a panel, a control panel in a
notebook, the Inspector's bench, a printed report.

```rust
pub struct Page {
    pub schema: SchemaId,                  // "dna.page/1"
    pub title: String,
    pub root: Node,
    pub input: InputMode,                  // Live, or Batched with the author's reason
    pub tab_order: Vec<ItemId>,
    pub print: Option<PrintLayout>,
}

pub enum Node {
    Stack   { children: Vec<Node> },                           // down
    Row     { children: Vec<Node> },                           // across; wraps when narrow
    Grid    { columns: u8, children: Vec<Node> },
    Section { title: String, folded: bool, children: Vec<Node> },
    Tabs    { tabs: Vec<(String, Node)> },
    Part    { id: ItemId, component: ComponentRef,
              bind: BTreeMap<SlotName, Bound>, props: Value },
}

/// A binding carries a stable identity and the exact address. Both must agree before it is used.
pub struct Bound { pub anchor: AnchorId, pub address: String }
```

- A component declares its slots: each slot's name, whether it reads or writes, and the kinds of
  target it accepts. A page is checked against those declarations before it is shown.
- A binding that does not resolve, or whose identity and address disagree, is shown as broken
  and blocks Keep. Nothing falls back to a nearby cell (Pro 21, U01 and U05).
- A page never holds a formula. It binds and quotes; the workbook computes. An agent that wants a
  new number proposes a cell for it.
- Layout is flow, not coordinates. Stacks, rows and grids reflow at 200% and 400% zoom and on a
  narrow screen (Pro R-56).

### 8.5 The command table and named commands

```rust
pub struct CommandSpec {
    pub id: CommandId,                     // "edit.fill-down": stable, namespaced
    pub label: &'static str,
    pub level: Level,
    pub excel: Option<ExcelCommand>,       // the Excel command it stands for
    pub args: ArgsSchema,
    pub help: HelpId,
    pub available: fn(&CommandContext) -> Availability,
    pub run: fn(&mut Invocation) -> Outcome,   // builds a batch or an operation, or opens a shell surface
}
```

One table feeds the Deck, the palette, the keymap, the key tips, the context menus, the agent's
tool list and the site's help pages. A view adds its rows through `ViewSpec::commands`. The 14
commands of today's `CommandId` (`command.rs:7`) are its first rows.

A skin's named command is a fixed sequence of rows of this table with fixed arguments. It has no
conditions and no loops. It runs as if the user had run each step, with the same previews and
the same refusals. It is not a macro and needs no macro permission.

### 8.6 Making, trying, keeping, sharing

- **Check.** `validate(&Skin, &Context) -> Vec<Violation>`, in the runtime: the schema, unknown
  tokens, the contrast pairs, the fixed roles, key conflicts and the count of departures from the
  profile, commands that do not exist, bindings that do not resolve, components not installed.
  Each violation is a plain sentence with the place in the declaration.
- **Make.** A person uses Customise, a surface of the shell: drag panels, edit the Deck, set
  tokens with the contrast check live, edit the keymap with a one-cell trial bench, or edit the
  declaration as text beside a live preview (Pro 21, U01 to U03). An agent writes the same
  declaration. There is one format and one check for both.
- **Try on.** A valid skin is applied in this window under a band: "Trying *Calm review* · Keep ·
  Edit · Discard (Esc)". Nothing is stored and the stamp does not move.
- **Keep** for me (the user's store) or for this workbook (`PresentationChange::Skin`: one history
  entry, undone with Ctrl+Z).
- **Share** as one text file. It holds no workbook values.
- **What a skin cannot hold goes the other way.** A row height is the workbook's formatting, so a
  request for taller rows is offered as a model change, not put in the skin.

## 9. The extension boundary

### 9.1 A file carries declarations only

The view part's schema has no place for code: no script, no expression, no address to fetch. Its
`requires` section names components by id and version. Opening a file never runs anything the
file brought and never fetches anything the file named, so a workbook with an App, a notebook
and a skin opens with no security prompt at all. A macro-free `.xlsx` stays a file that cannot
carry code.

### 9.2 Code enters as an installed, signed component

An extension is a package the user installs: a manifest, its files and a signature.

```json
{ "schema": "dna.extension/1", "id": "acme.yield-curve", "version": "1.2.0",
  "publisher": "Acme Analytics", "port": "dna.component/1",
  "components": [ { "id": "curve", "entry": "curve.html", "fallback": "data-frame",
                    "slots": [ { "name": "rates", "access": "read",  "accepts": ["cells", "table"] },
                               { "name": "tenor", "access": "write", "accepts": ["cells"] } ] } ],
  "network": [] }
```

- **Signed and installed like an add-in, behind the same trust card.** The card names the
  publisher, says what the component can see (what it is bound to) and what it cannot do
  (calculate, read the workbook, reach the network unless hosts are listed). The package's
  digest is checked at install and at every load. Nothing is installed from a workbook.
- **Sandboxed.** Each instance runs in an isolated browser frame (an `iframe`) with an opaque
  origin and a content policy that allows the package's own files and nothing else. It cannot reach the page, a port, storage or,
  by default, the network. The mechanism is the same in a browser and in WebView2.
- **It speaks one small port,** the only code boundary in the design that crosses versions at
  run time, and so the only one versioned by hand:

```rust
pub enum ToComponent {
    Init     { props: Value, theme: TokenValues, locale: Locale },
    Bind     { slot: SlotName, data: BoundData, stamp: Stamp, coverage: Coverage },
    Selected { slot: SlotName, item: Option<ItemKey> },
    Resize   { width: u32, height: u32 },
    Theme    { theme: TokenValues },
}

pub enum FromComponent {
    Ready  { wants: Size },
    Intent { slot: SlotName, value: CellValue },     // a write to one of its own write slots
    Select { slot: SlotName, item: ItemKey },        // move the selection to something it shows
    Window { slot: SlotName, rows: Range<u64> },     // page through a frame
    Help   { id: String },
}
```

- **Bound values come in with their provenance**, so a component can be as honest as the kit's
  own. An `Intent` becomes a batch that the hosting kit makes, with origin `Component`, under
  the cell's validation and protection, through the seam where the hosting view shows one. A
  component cannot ask a query, commit, run a macro, change a skin or see what it is not bound
  to.
- **When the extension is not installed,** the fallback the declaration names (a data frame, a
  value, a chart) draws the same binding, with a line saying what is missing and where to get
  it.

### 9.3 Add-ins are the other door, and stay apart

An add-in adds functions: calculation, at level 0, under the add-in and macro trust that exists.
An extension adds a component: presentation. An add-in cannot draw and an extension cannot
calculate. Neither trust covers the other, and the Guarded level of macro security says nothing
about native add-in code (Pro 21, E03).

## 10. Agents

### 10.1 The same grammar

An agent names targets in the address grammar, reads through the queries, runs rows of the
command table and writes declarations. It has no private door (Strand tenet 7). Its tool list is
generated from the contract: one tool for each query name and one for each command, with the
argument schema each already has. The tools cannot drift from the application, because they are
the application's tables.

### 10.2 Two lanes that never mix

| | Looks | Model |
|---|---|---|
| What the agent writes | A skin, a page, a view document: declarations | A proposal: batches of typed changes |
| How the user meets it | A try-on, under the band | A review, by unit |
| What accepting does | A presentation change; `model` does not move | Model changes, one history entry per unit |
| What it can never do | Grant a provider, a macro or a component anything | Arrive without a preview |

The two worked examples of the designs are these two lanes. "Restyle Classic" is a skin: checked,
tried on, and its "0 cells, formulas or names" is `Preview.kind`, computed. "Build Loan's App
from three lines" is a page: the agent reads `Structure` and `Rules`, binds inputs and outputs
by anchor and address, and the page is the try-on. Its one suggestion for the model ("define
four names?") arrives separately, as a proposal.

An agent hands a declaration to the user's shell with `Operation::TryOn`. The host stores
nothing and decides nothing. It passes the declaration to the page, and the shell runs the
check of 8.6 and shows the band. Keeping it is the user's act, with the user's origin.

### 10.3 Proposals

```rust
pub struct Proposal {
    pub author: String,
    pub base: Stamp,
    pub intent: String,
    pub units: Vec<Unit>,
}

pub struct Unit {
    pub id: UnitId,
    pub intent: String,
    pub batch: Batch,
    pub needs: Vec<UnitId>,        // a unit that uses a name needs the unit that creates it
    pub supported: bool,           // an idea that is not a change is listed, never applied
}
```

- **Preview is for the subset the user ticks.** Its writes and its effects are worked out for
  exactly those units, not for the whole proposal (Pro R-44).
- **`Decide` validates everything accepted before it writes anything.** If one accepted unit is
  stale, nothing is written and the conflicts are shown at their cells: what the agent saw, what
  is there now, what it proposed (Pro R-47). Otherwise each unit commits as its own history
  entry, in order, in one group. Ctrl+Z takes back the last unit only (Pro R-46). `needs` keeps
  half of a dependent pair from being offered alone (Pro R-45).
- **Under "apply, then review"** the same units are applied at once and their entries are marked
  unreviewed until kept or reverted. The panel and the verbs are the same.
- **Stop** leaves the proposal incomplete and unapplied. Reopening checks its base again. An
  accepted unit's identity is in the journal and cannot run twice (Pro R-49).

### 10.4 The trail

The agent's change trail is kept in the file (D14), in the `trail` section of the view part: who,
when, which proposal, which units and targets, and whether each was accepted, rejected or
undone. It is a summary, not a transcript. It can be turned off for a workbook, and turning it
off is itself a presentation change in the history.

### 10.5 Where an agent connects

- **Inside the application:** the Mast's field hands the request to the agent bridge, which is a
  second connection on the session with origin `Agent` and the policy the user chose.
- **From outside:** `dnacalc serve` already offers a loopback socket with a capability token. It
  carries the same envelope, so an outside agent is the same participant. A tool server over
  that envelope is a thin composition in the CLI.
- **In the repository:** an agent that writes a view writes Rust against `calc-skin` and the kit
  and runs the harness. That is how the five views get built.

### 10.6 The harness

The fourth part of the SDK the owner's scope page asks for. `dnacalc skin check <skin>
--workbook <file>` checks the declaration, opens the workbook under it in each view, takes a
screenshot of each in the three themes, runs the keyboard list, and compares the model before
and after, which must be equal. It returns a report an agent can read. The same harness runs
the scripted cases for us.

### 10.7 Asked, never assumed

Changing calculation settings, allowing macros, loading an add-in, installing a component, saving
and sending anything outside are operations an agent's port cannot run without the user's
answer (Fable R-87, Pro R-50).

## 11. One code base: web and desktop

### 11.1 The compositions

There is one page bundle. What differs is where the host runs and what carries the envelopes.

| Composition | The page runs in | The host runs in | Carrier | Calculation |
|---|---|---|---|---|
| Browser (`/app/` on the site) | the browser | a web worker, in wasm | worker messages; bytes as transferred buffers | the worker's own loop |
| Windows desktop | WebView2 | the desktop process, native | WebView2 messages | the window's thread, in 6 ms slices of an 8 ms tick, as today: add-ins and COM need that thread |
| `dnacalc serve`: any system, and the Linux and macOS desktop for now | the user's browser | the CLI process, native | a loopback WebSocket with a capability token | the server's loop |
| Tests and headless tools | a test page, or none | in process | direct calls | stepped by the test |
| Formula Explorer in a browser, and on the site | the browser | the page itself: level 0 in process | direct calls | none |
| Formula Explorer on the desktop | WebView2 | a native level-0 host; add-ins in process or in the surrogate | WebView2 messages | none |

All of these exist today for the old messages. `cfg(target_arch = "wasm32")` belongs in the kit's
adapters and in the compositions. A view has no platform code. The runtime and the contract are
portable and are tested natively and in both browsers, as AGENTS.md requires of portable
packages.

### 11.2 A difference is a capability, never a branch in a view

| | Browser | Windows desktop | `dnacalc serve` |
|---|---|---|---|
| Files | the browser's picker and download | native dialogs, recent files | through the host's file system |
| Clipboard | text and HTML | Excel's own formats as well | text and HTML |
| VBA | runs (calc-vba is portable), at the level chosen | runs | runs |
| .xll and COM add-ins, COM RTD servers | no: saved values, marked, with a route to the desktop | yes | no |
| The model clock | yes | yes | yes |
| Keys the platform keeps | the browser's | few | the browser's |
| COM automation of the application | no | yes | no |

One rule replaces today's special cases (`DesktopCommand`, the `__dnacalcDesktop` flag): a
platform service is an operation. The host performs it, or refuses it with `NotAvailableHere`,
and on a refusal the shell uses the page's own route where there is one (the browser's file
picker). `Capabilities` tells the shell in advance, so a control that cannot work here says so
before it is pressed.

### 11.3 The desktop shell stays thin

`apps/dna-calc-desktop` and `platform/calc-platform-desktop` are kept. The shell makes the window
chain that add-ins expect, hosts WebView2, carries envelopes without decoding them, and gives the
host its native services. COM automation keeps reaching the same session through
`calc_host::automation`. Its carrier should stop sending tile bytes as base64 text (17.2).

## 12. Persistence

### 12.1 One part

`urn:dnacalc:view-manifest`, version 2: one custom XML item with its properties part, as
version 1 has. One reader and one writer in calc-doc serve every view (the brief's item 6).

```xml
<manifest xmlns="urn:dnacalc:view-manifest" version="2" model="digest of what Excel reads">
  <prefer workbook="app"><sheet id="3" name="Summary" view="notebook"/></prefer>
  <supports>classic notebook app</supports>
  <anchors>
    <anchor id="a17" kind="cells" sheet="Analysis" ref="A10" name="PaidOrders" print="…"/>
  </anchors>
  <docs>
    <doc id="nb-analysis" view="notebook" scope="sheet:5" schema="dna.notebook/1">{ … }</doc>
    <doc id="app" view="app" scope="workbook" schema="dna.app/1">{ … }</doc>
  </docs>
  <skin schema="dna.skin/1">{ … }</skin>
  <readings>…</readings>
  <notes>…</notes>
  <trail enabled="true">…</trail>
  <requires><component id="acme.yield-curve" version="^1" fallback="data-frame"/></requires>
</manifest>
```

### 12.2 The sections

| Section | Holds | Changed by |
|---|---|---|
| `prefer`, `supports` | The workbook's preferred view, each sheet's, and the views the file was set up for. A preference is shown as a badge on a sheet tab and applied when the file is first opened, never by a click on a tab | `Prefer` |
| `anchors` | 12.3 | `Anchor`, and the host on structural edits |
| `docs` | Each view's document, as JSON the host does not interpret | `Doc`, `CreateDoc`, `RemoveDoc` |
| `skin` | The skin that travels with the file | `Skin` |
| `readings` | Confirmations and corrections of what the program inferred: the range, the kind, the state (from the workbook, suggested, confirmed by whom, needs review after a change) and the evidence it rested on | `Reading` |
| `notes` | Notes and findings (D5), each on an anchor | `Note` |
| `trail` | The agent's change trail (D14) | the host on `Decide`; `Trail` turns it on and off |
| `requires` | The components the documents use, with fallbacks | worked out at save |

- Typed XML for what the host must understand to keep it right: anchors, preferences, notes, the
  trail. JSON text for what it must not interpret: documents and the skin.
- An element or attribute this version does not know is kept byte for byte when the part is
  rewritten. That is the version 1 codec's rule, and it stays.
- A part of a version this build does not know is kept whole and never overwritten. The workbook
  opens in Classic with a notice (Pro 21, W05).
- A visible mirror of notes (cell notes for findings, a notes sheet for prose) is a model change
  offered at save and off by default (D5).
- A sheet is named by Excel's own `sheetId`, which Excel keeps through a rename and a reorder,
  with the name beside it for a reader. Preferences and a document's scope use it.
- The `model` attribute is a digest of every part of the package except this one and the
  document properties, taken when DNA Calc last saved. It is how an edit made elsewhere is
  noticed (12.3).

What the typed sections hold, entry by entry:

| Entry | Fields |
|---|---|
| A reading | Id, kind, the anchor it is over, state, who confirmed it and when, the fingerprint of the evidence it rested on |
| A note or a finding | Id, kind, the anchor it is on, text, author, time; for a finding, whether it is open or resolved |
| A trail line | Time, the agent's name, the proposal's id and intent, each unit's id, targets and outcome (accepted, rejected, undone), and who decided |

### 12.3 Anchors

```rust
pub struct Anchor {
    pub id: AnchorId,
    pub to: AnchorTarget,          // a name, a table part, an object by its name, a spill's first
                                   // cell, or cells by sheet and range
    pub print: Fingerprint,        // the labels beside it, the relative shape of its formulas, its size
    pub state: AnchorState,        // Bound, Relocated { from }, Orphan
}
```

- **Keyed to cells and to the user's own names, tables and objects.** Where a name or a table
  exists, the anchor is to it, and Excel itself keeps it in step. Where none exists, the anchor
  is an address with a fingerprint. No hidden name is ever added (D13). "Pin with a name" is the
  user's choice and a model change.
- **Edits made here** move anchors in the same transaction as the cells (5.2). An anchor whose
  target is deleted becomes an orphan and is kept, with a notice.
- **Edits made elsewhere.** The part carries a digest of the model as DNA Calc last saved it. A
  different digest on opening means the file was edited elsewhere, and every address anchor is
  checked against its fingerprint: found in place, it is bound; found moved, it is relocated and
  marked; not found, it is an orphan, kept and shown as broken. It is never moved to a nearby
  cell on a guess (Fable R-77).
- **A missing part** costs the views' authored state and nothing else. The model is intact, the
  workbook opens in Classic, and every view still works from readings (Fable R-106).

### 12.4 Version 1

Read: version 1's blocks and placements become version 2 documents, with anchors keyed to what
each hidden locator name referred to. Write: version 2 only. Removing the `_dna.block.*` names
changes what Excel reads, so it is not done silently at a save. It is offered once, as an
ordinary batch through the seam's review ("Update this file's view data: writes version 2 and
removes 14 hidden names that DNA Calc added"), and it is one history entry. Until it is
accepted the file keeps version 1, and its layout is not rewritten.

### 12.5 Round trips with Excel

What is promised is a tested list of save routes, not "Excel keeps it" (Pro push-back 4).
Opening and saving in desktop Excel as .xlsx and .xlsm comes first (dc-1nbp.22, in progress; it
needs Windows). A format that cannot carry the part (.xls) is refused, or the loss is named
before the save (Pro 21, O02). Excel's Document Inspector and other tools can strip the part;
12.3 says what that costs.

### 12.6 What is not in the file

Per user and per workbook, in the application's local store (the browser's storage, or the
user's profile folder on the desktop): each view's scroll and camera, panel sizes, the companion
pane, places, drafts kept for recovery, recordings, the user's own skins, installed components
and recent files. Classic's own window state (panes, zoom, the active cell) is Excel's and stays
in the file where Excel stores it.

### 12.7 The Formula Explorer's file and the state word

The Explorer's session file stays what it is: a plain .xlsx with one formula cell, the constants
around it and one custom XML item (`urn:dnacalc:onecalc-session`) for options and the add-in
list. No new file type.

The Mast's state word (new, unchanged, saved, edited, "saved, layout edited", read-only) is the
host's to say, in `Summary`. A stamp alone cannot say "unchanged" after an undo, because an
undo is a new revision. The host knows where the history stood at opening and at the last
save, so undoing back to that point reads "unchanged" (Fable R-34). `model` and `view` tell
"edited" from "layout edited".

## 13. Automation

### 13.1 Excel's object model, on the same session

VBA, COM clients and Office.js scripts reach `calc_host::automation` as they do today. Nothing in
this design changes what they can do.

- `Selection`, `ActiveCell` and `ActiveWindow` are the session's selection (4.2), so a macro's
  `Select` moves every view.
- `Application.StatusBar` text shows in the Strip, labelled as the macro's.
- `ScreenUpdating = False` holds frames back (4.6). It never hides "Macro running".
- `MsgBox`, `InputBox` and a user form arrive as a macro event with a dialog to answer, and the
  shell shows it over any view.
- A macro is started from the page with `Operation::RunMacro`: a form button, the macro list.
  The page cannot do this today.
- Macro security is the host's. Off, Guarded and Unguarded hold in every view, including behind
  a `Button` on an App page (Pro R-65). No skin, page or file can raise the level.

### 13.2 The views library for VBA

Three verbs: show a view, select by address, show or hide a panel.

```rust
pub enum ViewRequest {
    Show   { view: ViewId, sheet: Option<String> },
    Select { address: String },
    Panel  { id: String, open: bool },
}
```

- A view request is navigation. It goes on Back and Forward, not into the history, and it
  changes no calculation setting. It waits for an edit in progress. It cannot dismiss a trust
  card, hide the Strip or change a skin.
- There are two spellings of the one operation. From VBA it is late bound, so the same workbook
  still runs in Excel: `Application.Run "DnaCalc.View", "Notebook"`. In Excel that call fails in
  a way `On Error Resume Next` steps over. A referenced type library is never used: a missing
  reference stops a VBA project compiling in Excel. From a COM client that drives the
  application it is a `Views` object on DNA Calc's application object (section 19, point 3).
- No calculation may depend on a view call (Pro R-65).

### 13.3 Without a user interface

The CLI (`recalc`, `get`, `set`, `diff`, `run`) and the Rust crates use the same hosts. A script
or an agent that wants the engine and no view takes level 1 without a page. One that wants only
formulas takes level 0. A macro run's place in the history is in 17.1.

## 14. The website and the app

Today the site (`docs/site`, built by `cargo xtask site`) embeds the wasm Formula Explorer on
three pages and serves the whole web app at `/app/`. There are no links into a place in either
app, the Explorer box cannot be handed a formula, and no app source links to dnacalc.com.

### 14.1 One link grammar

```rust
pub struct Link {
    pub app: AppKind,                  // Calc or Explorer
    pub source: Option<Source>,        // a sample by its id; a file the user's own browser may fetch
    pub at: Option<String>,            // an address, in the address grammar
    pub view: Option<ViewId>,
    pub panel: Option<String>,
    pub trial: Option<TrialText>,      // Explorer: a formula, literal inputs, a format code
    pub skin: Option<SkinRef>,         // a published skin, to try on
}
```

- Its text form is a fragment, which is not sent to a server:
  `https://dnacalc.com/app/#src=sample:budget&view=notebook&at=Summary!B7`, and
  `https://dnacalc.com/explorer/#f=%3DPMT(6.5%25%2F12,360,-250000)`. The desktop application
  takes the same fragment through a `dnacalc:` scheme.
- A link opens a place. It carries no workbook data unless the user put it there: an Explorer
  trial's literals are data, and "Copy link" shows what the link holds before it is copied.
- A link is what Back and Forward hold, so "Copy link to this place" is the same object.
- A link to a skin leads to a try-on. No link installs a component, allows a macro or changes a
  setting.

### 14.2 From the site into the app

- **Every formula on the site is a link.** It opens in the Explorer box on the page, in the web
  app, or in the desktop application.
- **Every story is a workbook, a list of steps and a page.** The cast (Budget, Sales, Loan,
  Stock, Inherited) are real workbooks in the repository. Each step on a story's page has "Open
  this step": a link with a source, a view and an address. The same workbooks are the app's
  "New from a story" and the tests' fixtures, so the site, the app and the tests tell one set of
  stories.
- **Every published skin** is a file and a try-on link.

### 14.3 From the app back to the site

- **Help ids.** Every row of the command table, every function, every kind of refusal and
  notice, every view and every kind of reading has a help id, and the id is a page:
  `https://dnacalc.com/help/<kind>/<id>`. A function's page is its Handbook page. F1 and the
  palette open it. The list of ids is generated from the tables the app runs on, and the site's
  check fails when an id has no page.
- **Evidence.** The build record behind the site's status page (corpus and function-bank counts,
  from tool output) is embedded in the app. About shows the same numbers for the same commit,
  and the function browser's match status is read from the same witness table.
- **"Report a difference from Excel"** opens a pre-filled report: the formula, the inputs as
  bits, the build. The user reads it before anything is sent.
- **A route is a link.** When a capability is missing here, the refusal's route ("Open in the
  desktop app", "Open in the browser") is a link to the same place.

### 14.4 What this lets the site show

The same workbook in five views. The engine with no user interface: crates and a CLI. One page
in a browser, on Windows and behind `dnacalc serve`. An agent that reads, proposes and restyles
through one grammar, with a harness. Each is a link into something running, which is what the
book's new hook (D8) can stand on.

## 15. The code shape

### 15.1 Crates and modules

| Crate | In | Made from | Modules |
|---|---|---|---|
| `calc-skin` | `crates/` | New. The wire types of `dna-calc-model/src/projection.rs` and the tile codec of `dna-calc-protocol` are its starting material | `target`, `selection`, `stamp`, `change`, `batch`, `history`, `query::{formula, workbook}`, `watch`, `event`, `evidence`, `refusal`, `capability`, `port`, `carrier`, `stream` (tile types and codec), `page`, `skin`, `part`, `proposal`, `component`, `link` |
| `calc-formula-kernel` | `crates/` | New and small | The pure level-0 answers over calc-fml and calc-func |
| `calc-formula-host` | `crates/` | The pure files of `apps/dna-onecalc` (5,940 lines) and entry reading from calc-host | `session`, `trial`, `providers`, `rtd`, `format`, `file`, `serve` |
| `calc-host` | `crates/` | Kept (51,264 lines) | The seven additive changes of 5.2 |
| `calc-skin-host` | `crates/` | `apps/dna-calc-worker` without the per-view projections | `serve`, `queries` (a module per family), `watch`, `publication`, `proposals`, `platform` |
| `calc-doc`, `calc-doc-model` | `crates/` | Kept | The version 2 codec; `ViewPart` in place of `SkinLayout` |
| `calc-fml` | `crates/` | Kept | A new `lens` module beside `assist` |
| `dna-strand` | `apps/` | Kept (1,271) | The contrast check made callable |
| `dna-skin-runtime` | `apps/` | The logic of `dna-calc-model` (12,744 lines kept) | `store`, `mirror`, `edit`, `modes`, `input`, `commands`, `keymap`, `places`, `skin` (check and layering), `plan`, `geometry`, `print`, `local` |
| `dna-kit` | `apps/` | New | The basic components of 6.3, the page renderer, the registries, the `View` trait, the browser adapters |
| `dna-grid` | `apps/` | New, from the grid's pure parts | The grid as a component: panes, painters, hit testing, gestures, editing in the cell |
| `dna-formula-ui` | `apps/` | Kept (4,001) | Re-pointed from `dna-calc-model` to `calc-skin` and the runtime |
| `dna-shell` | `apps/` | New | The regions of 6.4, the seam, the palette, dialogs, file and print, trust cards, try-on |
| `dna-view-classic`, `-inspector`, `-notebook`, `-map`, `-app`, `-explorer` | `apps/` | New. Classic is `dna-grid` on the stage; `-inspector` also exports the instruments; `-app` also holds the Forms maker | One crate each, so that lanes can build views side by side and the checker can hold them apart |
| `dna-calc`, `dna-calc-worker` | `apps/` | Rewritten small | Compositions: the page with its registry; the web worker around `calc-skin-host` |
| `dna-calc-desktop`, `dna-onecalc-desktop`, `dnacalc` | `apps/` | Kept | The carrier's messages change; `dnacalc` gains `skin check` |
| `dna-onecalc` | `apps/` | Rewritten small | The Explorer's page: shell, one view, skin 0, level 0 |

`apps/dna-calc-model` and `apps/dna-calc-protocol` end: their contents are split between
`calc-skin` and the runtime.

### 15.2 What is kept

What was measured against Excel, or is proven by tests that still mean something.

| Kept | Lines | Why |
|---|---|---|
| calc-host and everything beneath it | 51,264 and more | The workbook's behaviour. Nothing here changes what a workbook calculates |
| Page logic in the model crate: key grammar, render plan, geometry, viewport, tile mirror, print layout, drawn shapes | 12,744 | Pure, tested natively and in both browsers, and where the grid's parity with Excel lives |
| The grid's pure parts: `panes`, `axis`, `decoration`, `alignment`, `autofit`, `painter` | about 3,800 | The same |
| The worker's modules behind the projections (5.3) and the flows of `server.rs` | 6,921 and the flows | Cell assembly, flow control, formats, search, print |
| The Formula Explorer's session and its pure files | 5,940 | Level 0 as it is |
| `dna-formula-ui`, `dna-strand` | 4,001; 1,271 | The formula field every editor uses; the tokens and their contrast tests |
| The desktop shell, the platform crate, the surrogate, the CLI | | They carry messages and already serve three carriers |
| The version 1 manifest codec's rules for keeping unknown content | | They are right, and version 2 keeps them |
| The scripted cases for Classic and the shell (at least 828 of the 933), and the Explorer's 121 | most of 18,132 lines of cases | They hold the behaviour the grid has today, much of it compared with Excel. They address `data-dnacalc` roles and the render-plan hook, which the new page keeps as a published test contract; about 70 selectors that name classes, ids or child positions are ported. The 105 cases that name one of the five old skins go with those skins, and each new view brings cases of its own |
| `apps/dna-calc/tests/workbook.rs`, the Explorer's session tests | 3,454; 9,424 | Host-level tests, re-keyed from the old messages to the envelope |

### 15.3 What is thrown away

| Thrown away | Lines | Why |
|---|---|---|
| `controller.rs`, `controller/`, `input_adapter.rs` | 7,018 | One struct of signals that every skin shares and extends. The runtime and the ports replace it |
| The five non-grid skins: `notebook`, `atlas`, `tree`, `canvas`, `ledger` | 3,729 | The five views replace them. What they showed folds into the Notebook, the Inspector's overview and audit table, the map and data frames. The 105 scripted cases that name them go with them |
| `grid/mod.rs` and the rest of the grid's Leptos code | about 5,100 | Gesture state written again over the runtime's modes and input queue |
| The dialog families: `charts`, `review`, `format`, `printing`, `page_setup`, `pivots`, `names`, `tables`, `sheets`, `files`, `addins`, `clipboard`, `commands` | 11,038 | Most talk to the wire themselves. They are written again as shell dialogs over ports and the kit. Their behaviour is held by the scripted cases |
| `app.rs`, `skin.rs`, `port.rs`, `workbook_command.rs`, `model_adapter.rs` | 3,222 | The old shell and the old command layer |
| The wire types and page state in the model crate | 5,883 | `calc-skin` and the runtime's store replace them |
| The per-view projections in the worker, and their handlers | 291 and the handlers | Views read neutral queries |
| The Explorer's page and its port: `surface.rs`, `panels.rs`, `bridge.rs`, `port.rs` and the two web entry files | 3,906 | One shell and one contract |
| The page's style sheets | 665 | Written again on the tokens |

Of the page's 34,157 lines, about 30,350 are written again and about 3,800 move. That is the
"large chunks" of D10, and it is nearly all of the page.

### 15.4 The switch

The new page is built beside the old one, in new crates. D10 rules out a migration by small
steps, and this is how the cost of that is held down:

- **Kept logic moves first, once.** The model crate's kept modules move into the runtime and the
  worker's kept modules move into `calc-skin-host`, with `git mv`. The old page and the old
  worker import them from there. So the two pages share everything that is kept, and only what
  is being thrown away exists twice.
- **The old page is frozen** from the day the contract lands: fixes only, no features.
- **The scripted cases run against both pages** from the first day the new Classic draws a cell.
- **One commit deletes the old page**, the old messages and the two ended crates, when the
  cases for Classic and the shell pass on the new page.

AGENTS.md says to extend and never duplicate. Two pages for a bounded time is a known exception,
made by D10, and the deletion commit ends it.

## 16. The build order

Each stage ends in something that runs and can be shown. No dates.

| | Stage | Delivers | Ends when |
|---|---|---|---|
| 0 | The contract and the test kit | `calc-skin` with the types of section 4; the in-process carrier; a recording host for tests; the cast workbooks as fixtures; the moves of 15.4; rules 1 to 7 in `architecture.json` | Contract tests pass natively and in both browsers |
| 1 | Level 0 and the Formula Explorer (D2) | `calc-formula-kernel` and `calc-formula-host`; the first kit components; the shell's frame at level 0 (Mast, stage, Strip); `dna-view-explorer`; Explorer links | The Explorer's session tests and its 121 scripted cases pass on the new page, and it replaces the Explorer on the site. The Explorer that exists stays published until then |
| 2 | Level-1 serving, the runtime and the shell's frame | The envelope on all four carriers; the `Cells` stream; `Summary`, `Geometry`, `Objects`, `Rules`, `Formula`, `Find`, `Stats`, `History`; preview and commit over the model vocabulary; frames; the two revisions; history entries with label, origin and kind. The shell at level 1: Bridge, panels, palette, the seam, sheet tabs, the registries. A thin Classic on `dna-grid`: draw, select, type, undo | `tests/workbook.rs` passes re-keyed; a headless script edits and undoes through a port on each carrier; the new page opens a workbook and edits a cell. Published beside the old page |
| 3 | Classic in full | The Deck with key tips; the modes; the gestures; the dialogs; the conformance table | The cases for Classic and the shell (at least 828 of the 933) pass in Chromium and Firefox. Then the switch commit |
| 4 | Instruments and the Inspector | Host: `Deps` grouped, `Issues` by root, `Sources`, `Contributions`, `Census`, `Inventory`, and `Structure` for runs and tables. Each instrument lands as its query lands | The Sales and Inherited storyboards run as scripts |
| 5 | The view part and the Notebook | The version 2 codec, anchors and their reconciliation; `Placement`, `Rows`; the Notebook with Jupyter's keys; the Notes instrument (D5) | The Sales notebook of Pro 21 can be authored, saved and reopened. The round trip through Excel is a Windows step (dc-1nbp.22) |
| 6 | App, Forms maker, skins, agents | The page renderer; App; the Forms maker; `Structure` with roles for inputs and outputs; the skin check, try-on and keep; proposals, `Decide` and the trail; the agent bridge; `dnacalc skin check` | The two worked examples run end to end; the Loan storyboard; the proposal storyboard (accept A and B, undo B, the conflict on a changed cell) |
| 7 | The Model map | `Structure` by level; the map as a reader; the model clock wired in; then edits on named ports | The Stock storyboard as a reader, then with a rewire |
| 8 | Extensions, the views library, macro runs | The component port and sandbox; `ViewRequest` from VBA; `RunMacro` from the page; run groups; the experiments of 17.1 | A sample extension, installed and missing; a macro that switches view in DNA Calc and still runs in Excel |

- **Stages 4 to 7 run side by side** once stage 2 is done, because the shell's frame, the
  panels and the registries exist by then: one crate for each view. Classic does not wait for
  them and they do not wait for Classic. What they share is the kit, which is why it is kept to
  small parts (6.3).
- **Published early (D10).** From the end of stage 2 the new page is served beside the old one,
  and each view is published when its first version works. The switch in stage 3 changes which
  page is the default. It is not the first time anyone sees the new one.
- **The contract is not frozen at stage 0.** Page and host are one build (4.9), so the contract
  changes freely while its first clients are written. The thin Classic of stage 2 is there to
  find what only the grid's gestures show, before five views depend on it.
- **Site links and help ids** (14) start in stage 1 with the Explorer and grow with each stage.
- **What is hours:** a first working version of a view, by one lane, once its queries answer.
- **What is not hours:** Classic with its dialogs and the conformance tail; `Structure` and
  grouped `Deps` on large workbooks; reconciling anchors; capturing the key table from Excel,
  which needs Windows. Each gets its own beads after the book (D10).

## 17. The hard parts

### 17.1 Undo across a macro run (D4)

**What Excel does.** Running a macro clears the undo stack. `Application.OnUndo` lets a macro
register one routine of its own, and `Application.Undo` takes back the user's last action.

**What exists.** Automation writes enter one command at a time through `CommandIngress`, and each
is its own undo step. Nothing groups a run. The page cannot start a macro.

**From the first version.** A run is a group. `begin_run` is called when a procedure is entered
from outside (a button, an event, `RunMacro`, `Application.Run`), and `end_run` when the
outermost one returns. Every commit inside carries `Origin::Macro` and the group, and the history
shows one entry, "Macro: Roll_Month_End", with the cells it changed, which the host knows because
they are its own commits. That entry is `Irreversible`, and as in Excel the entries before it
cannot be reached by Undo. Before a run that can write, the shell offers a checkpoint: a pinned
root, which copies nothing when it is taken because pages are shared, restorable as a
whole-document action that says what it does not restore (Pro R-66).

**The experiment.** Make the entry `DocumentOnly`: pin the root at `begin_run`; on Undo, restore
it as a new transaction and recalculate. The restore path exists for structural edits
(`command.rs:1240`).

**Why it is deep.**

1. *A macro's own state is not document state.* Module-level and static variables, object
   references and open files outlive the restore and no longer match the cells.
2. *Outside effects cannot be undone:* files written, COM calls, add-in commands, the network,
   the clipboard. They can only be listed, and only as far as the host saw them. Under Guarded
   the audit log sees the capabilities it polices. Under Unguarded the entry must say that
   outside effects were not observed.
3. *Events.* A restore changes cells. Excel raises `Worksheet_Change` on an Undo. What our
   restore raises, and what happens when a handler writes again inside it, has to match Excel
   where Excel has behaviour and be decided where it has none.
4. *Application state the macro changed:* calculation mode, iteration, `EnableEvents`, the active
   sheet. Each is document state or session state, and each needs its own answer.
5. *Macros that use undo themselves.* `Application.Undo` and `Application.OnUndo` must keep
   Excel's meaning whatever we add.
6. *A run that is interrupted* leaves a partial group. It is still one entry, marked interrupted.
7. *Cost.* A root restore rebinds the graph. On a large workbook that is a full rebuild.

**What to test before anything is promised:** the cost of pin and restore on a workbook the size
of Inherited; the events a restore raises against Excel's; a module's state after a restore;
`Application.Undo` inside a handler; how complete the outside-effect list is under Guarded.

**Promised in the first release:** grouping, attribution, the list of what changed, the
checkpoint offer. Not promised: Ctrl+Z through a macro.

### 17.2 Big workbooks

The budgets are the designs' (Pro §6, Fable §6): a first viewport within a second from saved
values, a keystroke echoed within 50 ms during loading and recalculation, a view switch with the
selection in place within 150 ms. They are targets to measure and report, never a per-commit
check.

What the contract gives: a coverage on every answer; `Census` before anything is expanded; `Fit`
before a view mounts; budgets on tracked watches; `OverBudget` with ways forward; cancellation.

What stays hard:

1. **Staged opening.** Today a workbook is loaded and its engine state built before the first
   cell is projected. A first viewport before the dependency analysis is done needs the open
   split into stages, with answers that say `Partial` while loading. That is host and codec work.
2. **Grouped dependencies and structure at 300,000 formulas.** `Deps` by run and block and
   `Structure` need an index of runs of like formulas and of blocks, built in the background
   within a budget and kept current under edits. calc-fml's `CellRun` and calc-doc's regrouped
   formula sources are a start, not the index.
3. **Memory on the cell route.** A root still exists more than once between the host and the
   page. In a browser the host is 32-bit wasm with a 4 GiB ceiling. The largest Enron workbooks
   are the test.
4. **Bytes on the desktop carrier.** Tile bytes cross WebView2 as base64 in a string. A shared
   buffer or a binary channel is a change inside the carrier and touches nothing else.
5. **Watch footprints.** Recording what each watched query read has to cost less than running
   it again.
6. **Element counts outside the grid.** Data frames show a window, the map holds to its budget,
   a long notebook mounts only the cells in view.
7. **Calculation on the window's thread** on the desktop. Typing stays responsive in 6 ms
   slices. A long add-in call still blocks.

The test tier is the brief's item 8: corpus workbooks opened in every view, with no crash or
hang, the budgets recorded, and no value changed by any view. `Stamp.model` makes the last one a
one-line check.

### 17.3 Conformance with Excel's keyboard and mouse

The oracle is Excel on Windows: a pinned build, an English interface, a US keyboard. D11 extends
the bit-match rule to the keyboard. The four Pro 19 appendices are inputs and disagree in
places. The table is filled from Excel itself, and each row cites the build it was seen on. That
capture is a Windows task.

1. **The long tail.** Every missing gesture reads as a bug. The parity table has 31 rows and 21
   are partial today. The dialogs are part of it: Format Cells, Paste Special, Find and Go To
   Special each have keys of their own, and all are rebuilt.
2. **Modes.** Ready, Enter, Edit and Point, with F2 between them; End mode; Extend (F8); Add
   (Shift+F8); Scroll Lock. An arrow key means something different in each. Today's code has
   one flag.
3. **Type-ahead.** Keys typed during a calculation or a round trip must land in order (6.1).
   Ctrl+arrow depends on cells outside the mirror, so it is a round trip.
4. **Pointing** across sheets while a formula is edited; F4 on the reference at the caret.
5. **The mouse.** The fill handle (drag, double-click, right-drag); drag-move and drag-copy with
   Ctrl, with Shift to insert and with Alt to another sheet; scrolling at the edge while
   dragging; resize and double-click fit; hit targets a pixel apart that mean different things.
6. **Text input.** An input method's composition and dead keys begin an entry. On Windows,
   AltGr arrives as Ctrl+Alt, and on several keyboards AltGr with a digit types a character. So
   D12's Ctrl+Alt+1…5 must not fire on a keystroke that is typing one. The palette is the
   fallback D12 names, and the alternate for those keyboards is the book's to choose.
7. **Keys the platform keeps.** Browsers keep Ctrl+W, Ctrl+T, Ctrl+N, F11 and sometimes Alt.
   WebView2 hands nearly everything to the page. The difference is stated for each platform,
   with alternates, and never hidden.
8. **Key tips.** Excel's paths are long, change with context, and its older menu keys still
   work. The path table has to be captured, not written from memory.
9. **Focus.** The F6 ring, dialogs, panels. An agent's panel must never take a key meant for
   the grid (Pro R-16).

What the design does about it: one table drives both the keymap and its tests; the runtime
holds the modes and the queue, so every view inherits them; the 933 cases carry over; and
"Report a difference from Excel" is in the palette.

### 17.4 Previewing effects

"Which values would change" for a batch that is not committed needs a trial calculation. There
are two ways. One commits to a pinned copy and a second engine state, calculates within a
budget, compares and drops it: persistent roots make the document half cheap, and the engine's
results are the cost. The other commits, calculates and undoes, which macros, events and
watches would see. The first is the one to build. A proposal's effects are worked out only for
the subset the user ticks, on demand, and may come back partial. Stage 3 does not need it;
stage 6 does.

### 17.5 The clipboard

On the desktop the clipboard carries Excel's own formats; a browser offers text and HTML behind
a permission. Copy must put the thing on the clipboard and not a picture of it: cells with their
origin, so that relative references adjust (Pro R-19). The copy outline's lifetime is Excel
behaviour to match. Paste planning stays in the host, where it is today.

### 17.6 Anchors after edits made elsewhere

A fingerprint can fail both ways: a block that moved and also changed is not found, and two
blocks that look alike are found twice. The rule is that an ambiguous match is an orphan and
never a guess. The matcher needs testing at corpus scale, and the round trip needs Excel
(dc-1nbp.22).

### 17.7 Two pages for a while

Until the switch there are two pages in the tree (15.4). The old one can rot, and the new one
can drift from the cases. What limits that: the old page is frozen, kept logic is moved and not
copied, the cases run against both, and one commit ends it.

### 17.8 The component sandbox

A frame for each instance costs memory, and a page of many small components needs a budget, and
later perhaps one frame for many parts. Who signs packages and how a publisher's key comes to
be trusted is a product decision not yet made. Until it is, only packages the user installs
from their own disk and trusts by hand.

### 17.9 Printing and accessibility

Classic's printed pages must break where Excel's do (Fable R-95). Pagination is in the kept
print layout and has known gaps. Other views print through a page's print layout, from a frozen
snapshot. A painted grid needs the semantic projection the render plan already builds as the
screen reader's grid, with real coordinates under virtualisation (Pro R-55). Each kit component
carries its role once, so views inherit it.

### 17.10 The selection, and two writers, across the wire

The host owns the selection and the page moves it first (4.2). The sequence and the cause on
every selection event are what keep the page from taking a macro's `Select` for its own echo,
or from dropping a move the user made while a round trip was open. Classic's modes and
type-ahead rest on this, and it is where a fault shows only under load. It needs tests that
interleave keys, a macro and an agent on a slow carrier.

A user and an agent can also write at once. Model batches are protected by `base` and
`expects`. A view document is protected by the revision on each patch (4.4), which refuses the
second writer and does not merge. Merging two layout edits is not attempted.

### 17.11 A contract that meets its hardest client late

Thirty queries, the change vocabulary and the envelope are written before the grid uses them.
What limits the risk: the contract is not frozen while page and host are one build; stage 2
includes a thin Classic; and where an old message survives, the new one is a re-keying of it
(Appendix B), so most of the contract restates what already works. What is new and unproven is
named: `Structure`, grouped `Deps`, `Placement`, `Contributions`, and effects.

## 18. Not taken

| Not taken | From | Why not |
|---|---|---|
| A projection built in the host for each view | Today's code | Every new view then needs a host change and a wire change. Neutral queries instead |
| A presentation history for each view, with Ctrl+Z taking the more recent of two | Fable Pro 19 §3, R-89 | The owner's ruling is one history. The Strip names what was undone and where |
| Hidden defined names as anchors | Manifest version 1; Carbon's opt-in dual anchors | D13 |
| Level 0 as a tier inside calc-host behind a feature | The brief's option a | Features unify across the workspace. A crate boundary is checked and holds |
| FerroUI or another native toolkit now | dc-etcxb.1 to .3 | D7. Kept reversible: the contract and the runtime name no toolkit |
| Component code inside a workbook | The scope page's "code is used only for new components", read loosely | The README's agreement and all four designs: declarations only |
| Installed views with a query port, in the first version | A natural next ask | A narrow port first. A whole-stage component covers the need. Widen it with evidence |
| A separate read mode for the notebook | Pro 21's Author and Read | D6: read-only is how a file was opened, not a presentation |
| A notebook kernel with state of its own, and cells that run once | Jupyter taken literally | The one-evaluator rule, and the owner's scope page: live recalculation, not run-once. The workbook's calculation is the kernel |
| A tick, a director or a simulation step in the contract | Carbon's Directors; Neon's Run on a front-end F9 timer | D3: a tick is an Excel mechanism, the RTD clock or a macro |
| Confidence percentages on readings | Neon §5.1 | Four states with evidence, and no invented numbers (Pro §5; Pro 21, W02) |
| A new file type for Explorer sessions | Carbon's `.dnafe.json` | A plain .xlsx, as now (Fable push-back 6) |
| Agent edits applied unseen by default; unattended runs writing a branch copy | Carbon 11.2 | Propose by default. "Apply, then review" is the user's choice. The CLI writes where the caller says |
| Free tiling of many panes; a tab for each view | Considered in Fable §3 | People get lost; a tab hides that it is one workbook. At most two panes |
| Replacing calc-host's own command type with the contract's, now | "One of everything" | A large change to a crate automation also uses, for no gain in the UI. One translation, in one place (section 19, point 1) |
| Atlas, tree, canvas and ledger as top-level views | Today's six skins | Their uses fold into the Inspector, the map and frames (Pro §10) |
| Drawing workbook charts on the page | The implementation brief, item 3 | A chart is workbook content with one renderer, which also serves export and print (5.3) |
| Relying on the browser's Keyboard Lock for reserved keys | Neon 11.2 | Nothing should depend on a permission. Differences are stated, with alternates |
| A free-form query language over the workbook | A common way to avoid a fixed list | A typed query has a budget, a coverage and a test. A language has none of the three |
| A separate custom XML part for each concern | Possible reading of D5 and D14 | One part, one version, one loss check. Sections can be dropped one at a time |
| A host that answers the contract from a running Excel | The owner's note on linked boxes | Not now. The contract keeps it possible (4.6) |
| Named commands with conditions or scripts | Neon's skin commands, stretched | No language in a declaration. A sequence of commands is enough |
| Switching view automatically on a click on a sheet tab | The brief's "views can differ per sheet", read literally | A preference with a badge (the README's agreement) |
| Migrating the page in small steps | The implementation brief's ground rule "every step keeps the six current skins working" | D10 |

## 19. Open points for the merge

1. **One vocabulary or a translation.** calc-host could take `ModelChange` as its own command
   type. That is one vocabulary instead of two, and a larger change. Proposed: translate in
   `calc-skin-host`.
2. **One contract crate or two.** Proposed: one crate with a module for each level. Two crates
   would mean the Explorer cannot even name a level-1 type.
3. **How VBA spells the view verbs.** Proposed: `Application.Run "DnaCalc.View", …`. The other
   candidate is a late-bound object.
4. **Deleting a notebook cell that was placed from an existing sheet.** Proposed: remove it from
   the notebook only. Jupyter's own behaviour is to delete it.
5. **Rejected proposals in the trail.** Proposed: kept, one line each.
6. **A keymap in a workbook's skin.** Proposed: allowed, because it is only ever offered.
7. **Where the kernel's line runs**, if the two proposals cut level 0 differently.
8. **Who signs components**, and how a publisher's key comes to be trusted (17.8). This one is
   the owner's.

## Appendix A. What the designs need, and what meets it

The C-needs of the four Pro 19 answers, grouped. "Pro" is GPT-6 Pro's list.

| Need | Sources | Met by |
|---|---|---|
| 1. Identity that survives edits and reopening | Pro C-1; Fable C-43; Carbon C-15; Neon C-27 | `Target`, `Resolve`, anchors (4.1, 12.3) |
| 2. Exact content: formulas, values, formats, spills | Pro C-2; Fable C-20, C-29; Carbon C-12; Neon C-17, C-31 | `Cells`, `Geometry`, `Format` |
| 3. Value state and revision | Pro C-3; Fable C-2, C-3; Carbon C-11; Neon C-7 | `Stamp`, `Provenance` (4.3) |
| 4. Being told what changed | Pro C-4; Fable C-4; Carbon C-4; Neon C-5 | Frames, watches, `Changes` (4.6) |
| 5. Staged opening; empty, not loaded and failed apart | Pro C-5; Fable C-1; Carbon C-1; Neon C-1 | `Coverage`, `Summary`, notices |
| 6. Inventories and quick counts | Pro C-6; Fable C-31, C-48; Carbon C-2 | `Inventory`, `Census` |
| 7. Dependencies, summarised, with exact membership | Pro C-7, C-8; Fable C-8 to C-11; Carbon C-3, C-6; Neon C-11, C-14 | `Deps`, `Structure` |
| 8. Formula structure, the lens, exact rewrite | Pro C-9; Fable C-12, C-13; Carbon C-7; Neon C-10 | `Formula`, `Analyse`, `Present`, `Interpret`, `ReplaceReference`, `RenameName` |
| 9. What may be edited | Pro C-10; Fable C-27, C-30; Neon C-19 | `Rules` |
| 10. Preview and atomic apply; a stale base refused | Pro C-11, C-12; Fable C-17, C-18; Carbon C-16; Neon C-6, C-20 | `Batch`, `Preview`, `commit`, `Refusal::Conflict` (4.4) |
| 11. One history with attribution; revert; exact comparison | Pro C-13, C-33; Fable C-40, C-41, C-42; Carbon C-17 | The history (4.5), `Stamp.model` |
| 12. Recovery of work and of drafts | Pro C-14; Neon C-4 | The journal (kept); the runtime's local drafts (6.1) |
| 13. Trial and step-by-step evaluation | Pro C-15; Fable C-14; Carbon C-8, C-9; Neon C-9 | `Evaluate`, `Explain`, `TrialFor` |
| 14. Providers: safety, metadata, loading | Pro C-16, C-17; Fable C-36, C-37; Carbon C-18; Neon C-23 | `Providers`, `Capabilities` |
| 15. Formats as something evaluated | Pro C-18; Fable C-28; Neon C-18 | `Format` |
| 16. Calculation settings, cycles, convergence, observed order | Pro C-19, C-20; Fable C-5, C-6, C-7, C-19, C-21, C-22; Carbon C-10; Neon C-8, C-12 | `CalcState`, `CalcEvents`, `Evidence` (4.7) |
| 17. Outside data | Pro C-21, C-22; Fable C-35; Neon C-21, C-22 | `Sources`, source events |
| 18. Rows, contributions, error roots | Pro C-23; Fable C-15, C-16; Carbon C-5, C-20; Neon C-13, C-15, C-32 | `Rows`, `Contributions`, `Issues` |
| 19. Workbook objects and their settings | Pro C-24; Fable C-23 to C-26, C-32, C-34; Carbon C-13, C-22; Neon C-26 | `Objects`, `Inventory`, the model vocabulary |
| 20. Printing from one consistent snapshot | Pro C-29; Fable C-33; Carbon C-21; Neon C-25 | `Print`; asking never disturbs (4.7) |
| 21. Macros | Pro C-25; Fable C-38, C-39; Neon C-24 | `Macros`, `RunMacro`, macro events, `ViewRequest` (13) |
| 22. The view part: kept, versioned, loss detected | Pro C-26, C-27; Fable C-44, C-45; Carbon C-14; Neon C-3 | Section 12 |
| 23. The clipboard | Pro C-28; Fable C-46 | `Copy`, `PreparePaste`, `Capabilities` |
| 24. Accessible surfaces | Pro C-30 | The kit's roles; the semantic projection (17.9) |
| 25. Bounded, cancellable work; estimates first | Pro C-31, C-32; Fable C-50; Carbon C-19; Neon C-2 | Budgets, `Coverage`, `Fit`, `OverBudget` |
| 26. Readings with their evidence | Fable C-47; Neon C-16 | `Structure`; the `readings` section |
| 27. Agents | Fable C-49; Neon C-29, C-30 | The agent's port, proposals (10) |
| 28. Unsupported is not invalid | Pro C-34 | The kinds of `Refusal` (4.10) |
| 29. A place for new content | Neon C-28 | `Placement` |

## Appendix B. The old messages and what replaces them

Counted by script on this tree: 72 intents, 77 requests, 8 events.

| Old messages | Count | Become |
|---|---|---|
| Intents for entry and cells: `EnterCell`, `EnterSelection`, `Clear`, `ApplyPaste`, `Copy`, `Replace`, `ShiftCells`, `EditAxis`, `FitContents`, `AcceptValidation` | 10 | `ModelChange` in a `Batch` |
| Intents for format and structure: `FormatCells`, `AxisFormat`, `Outline`, `SplitPanes`, `FreezePanes`, `EditSheet`, `EditName`, `Table`, `AutoFilter`, `SortRows`, `SortRowsWithOptions`, `EditConditional`, `PageSetup` | 13 | The same families of `ModelChange` |
| Intents for review, charts, shapes, sparklines, pivots and refresh | 4, 10, 14, 4, 5 | `ModelChange::{Review, Chart, Shape, Pivot}`; `Operation::Refresh` |
| `NotebookLink`, `NotebookOrder` | 2 | `PresentationChange::Doc` |
| `Undo`, `Redo`, `Recalculate`, `Rebuild`, `CalculateRange`, `Calculate`, `SetCalcMode`, `SetIteration` | 8 | `Operation::{Undo, Redo, Calculate}`; settings as `ModelChange::Settings` |
| `Select`, `Navigate` | 2 | `port.select`, `port.navigate` |
| Requests that are one skin's projection: `Atlas`, `AtlasRefusals`, `Outline`, `Notebook`, `BlockDetail`, `Dependents`, `Canvas`, `Ledger` | 8 | Gone. Views read `ViewPart`, `Deps`, `Structure`, `Census`, `Issues`, `Rows` |
| `Subscribe`, `SetInterest`, `Unsubscribe`, `TileAck`, `ResetTiles` | 5 | A watch on `Cells`, its drop, its acknowledgement |
| `Geometry`, `UsedArea`, `Charts`, `Shapes`, `Sparklines`, `Controls`, `PivotTables`, `PivotSourceFields`, `CellImage`, `ShapePicture` | 10 | `Geometry`, `Objects` |
| `Names`, `EvaluateName`, `CalculateName`, `PollNameCalculation`, `CancelNameCalculation` | 5 | `Inventory`; `Explain` on a name, budgeted and cancellable |
| `EvaluatePart`, `TraceFormula`, `UserFunctions` | 3 | `Explain`; level-0 `Catalogue` |
| `Find`, `GoToSpecial`, `SelectionStats`, `CanEdit`, `ResolveReference`, `FilterValues`, `Validation`, `ValidationInvalid`, `Review`, `ReviewMarkers`, `FormatSelection`, `PreviewFormat`, `FitContents` | 13 | `Find`, `Special`, `Stats`, `Rules`, `Resolve`, `Rows`, `Issues`, `Cells` with detail, level-0 `Format` |
| `New`, `Open`, `OpenNative`, `OpenForEditing`, `InspectTemplate`, `OpenTemplate`, `InsertTemplate`, `Save`, `SaveWithContext`, `SaveAs`, `SaveCalculated`, `PollSave`, `TakeMacroFile` | 13 | `Operation::{New, Open, Save}`, with progress as events |
| `Capture`, `PreparePaste` | 2 | `Operation::{CopyToClipboard, PreparePaste}` |
| `LoadVbaModule`, `LoadVbaProject`, `SetVbaEventsEnabled`, `EnableMacros`, `UnblockMacros`, `SetMacroSecurity` | 6 | `Operation::EnableMacros` for the workbook's project; the rest are level-0 operations |
| `Addins`, `LoadAddinNative`, `LoadAddin`, `UnloadAddin` | 4 | Level-0 `Providers` and provider operations |
| `PageSetup`, `PrintPreview` | 2 | `Print` |
| `Summary`, `Command`, `Intent`, `Ping` | 4 | `Summary`; `commit`; the carrier's health |
| `AutomationBatch`, `AutomationRelease` | 2 | `Operation::Automate` |
| The 53 replies | 53 | Each query's own typed answer |
| The 8 events | 8 | A `Frame` of `Event`s; the three tile events stay inside the `Cells` stream |
| The Explorer's 32 requests and 17 replies | 49 | Level-0 queries and operations |

## Appendix C. Guarantees, and the test that proves each

| Guarantee | What gives it | The test |
|---|---|---|
| A view cannot change a calculation | Rule 7; a port's model changes are Excel's (4.4) | The dependency check; every kind of `ModelChange` saved, opened in Excel without repair and compared (Fable R-36; a Windows step) |
| A presentation change changes no value, formula, name or format | A `PresentationChange` can write only the view part; `Stamp.model` | 1,000 random presentation operations in each view leave `model` and the saved model parts as they were (Fable R-37) |
| Switching view changes nothing | The selection, the draft and the history are the session's and the runtime's; a view holds none of them | Switch through every view: the stamp, the selection and the draft are equal before and after (Pro R-1, R-3, R-4) |
| The seam tells the truth | `Preview.writes` comes from a prepare; `kind` is computed | The seam's list equals the journal after the commit (Fable R-35) |
| One undo for every view | One history, in the host | Each storyboard's undo, run from a different view (Pro R-21) |
| A stale apply never overwrites | `base` and `expects`, checked in the host | The 120, 123, 125 case (Pro R-47); an Explorer apply after an edit (Pro R-38) |
| A shown value is never a bare value | `Shown` and `Answer` are the only carriers | By type; each kit component tested under every provenance |
| An order of evaluation is never invented | `Evidence`; `CalcEvents` comes only from the host's own record | An instrument on a host with no record shows that there is none (Pro R-10) |
| The Explorer's build holds no engine | Rules 3 and 4 | The closure check in the sweep, for the desktop and the wasm build |
| Opening a file runs nothing from it | The view part's schema has no code; components are named by id | A workbook that names a component that is not installed: the fallback draws, nothing is fetched, nothing is asked |
| A skin cannot hide a status or weaken contrast | The fixed list (8.2); the check before a skin is applied | The 61 pairs under every published skin; a skin that sets a signal token is refused |
| Type-ahead is exact | The input queue (6.1) | Scripted keys sent faster than the replies give the same result as the keys sent one at a time |
| An agent cannot write unseen | The port's policy | Under the default policy, `commit` on an agent's port is refused and the history shows `Agent` only through `Decide`. Under "apply, then review", every entry an agent made is marked unreviewed until it is kept |
