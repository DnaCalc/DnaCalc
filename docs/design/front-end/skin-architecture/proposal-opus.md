# DNA Calc skin architecture: proposal B (Opus)

Bead dc-uxiu1 · 2026-11-26 · Opus 5.5 · written independently of proposal A.

This proposal defines the interface every DNA Calc view is written against, and the code shape
behind it. The owner rulings in [DECISIONS_20261126.md](../DECISIONS_20261126.md) override
everything here. Where this text says "the design sources", it means the Pro 19 answers (GPT-6 Pro,
Fable, Carbon, Neon), the Pro 21 package (73 scenes) and our own scope and brief
([FRONT_END_SCOPE.md](../../../plan/FRONT_END_SCOPE.md),
[FRONT_END_IMPLEMENTATION_BRIEF.md](../../../plan/FRONT_END_IMPLEMENTATION_BRIEF.md)). Code
references and line counts are at master `4120ba262`; no file under `apps/`, `crates/`,
`platform/` or `tools/` changed between `694a3134a`, where the reading started, and that commit.

**How to read it.** Section 1 is the whole answer on one page. Sections 2 to 14 define the
architecture, with Rust signatures where they matter. Section 15 is the code shape (what is kept,
what is thrown away), section 16 the build order, section 17 the hard parts, section 18 what is
not taken and why, section 19 the questions for the owner. Section 20 traces Pro 19's 34 C-needs
onto the contract.

---

## 1. The answer on one page

1. **A skin is data; a component is code.** A skin is a manifest: its root component, which
   components fill which regions, which panels open, the tokens, the keymap layer, the command
   placements and the schema of the skin's per-workbook declarations. The built-in skins are the
   five views (Classic, Inspector, Notebook, Model map, App), the forms maker (the view that
   authors App pages) and the Formula Explorer (skin 0). A skin made by a user or an agent is a
   manifest *based on* a built-in one. Only components are code, registered by id: built-in ones
   in the component library and the skin crates, or signed installed extensions. A workbook file
   carries manifests and declarations, never code.
2. **One contract, one crate.** Everything a skin may read or change is defined in one portable
   crate, `calc-skin-api`: targets and addresses, the one selection, one command vocabulary in four
   kinds (model, presentation, calculation, navigation), transactions with origin and undo units,
   queries with budgets, coverage and provenance, calculation status and evidence, and events. The
   page, the wire, the service and calc-host all use these same types. Today's four layers of the
   same operation (`WorkbookCommand` on the page, wire `Intent`, the worker's 1,200-line
   translation, calc-host's `SkinIntent`) become one enum.
3. **One history, in the host.** Presentation edits are host transactions on the workbook's view
   part, exactly as model edits are transactions on cells. So there is one ordered undo history,
   one save, one dirty flag, and "presentation only" is provable: the model fingerprint does not
   change. The mechanism exists today: the skin layout is document metadata in `ModelRoot`, and
   its edits are workbook operations that calc-host's history already reverses
   (`WorkbookOp::EditSkinLayout`, `crates/calc-host/src/command.rs:202`). What changes is the
   content: anchors without hidden names and per-skin declarations instead of v1 blocks.
4. **Two host levels, two crates.** Level 0, the formula host (`calc-formula-host`), evaluates
   formulas without a workbook: trials with input cells, the catalogue and help, conditional
   formats, add-in providers, a VBA module's UDFs and RTD topics. It is the Formula Explorer's
   existing `Session`, moved below the page. Level 1, the workbook host (calc-host), owns a level-0
   host and adds cells, the graph, recalculation, history and subscriptions. A crate boundary, not
   a feature, keeps calc-engine out of the Formula Explorer. Completion runs on the page over the
   catalogue, and number formatting is calc-func's one function on both sides.
5. **A kit makes a view a few hours of work.** `dna-skin-kit` is the client of the contract on the
   page: a `Book` handle whose reads are live values (`Live<T>`, each with revision, provenance and
   coverage), one command bus, the keymap engine with Excel's input modes, focus and input
   routing, the view host (primary view, companion, panels), typed access to the view's
   declaration, and a test probe. A view author declares what it reads, composes components and
   maps gestures to commands; selection, undo, status, persistence, printing hooks, command search
   and accessibility announcements come from the kit and the shell.
6. **Beauty is systematic.** All styling lives in one Leptos component library on Strand tokens
   (`dna-ui`); skins write no CSS. Tokens resolve at run time with the contrast law checked, so a
   skin's theme can change without a rebuild and cannot break a sanctioned pair or recolour an
   error.
7. **Fresh UI, kept logic.** The current skins, the controller, the page shell, the per-skin worker
   projections, the wire intent layer and the Formula Explorer's own port and shell are thrown
   away. The DOM-free logic that holds hard-won Excel behaviour is kept: selection grammar, the
   edit session and point mode, geometry, the render plan, tile publication and mirrors, the grid
   painter, printing pagination, drawn-shape geometry, and the 1,053 browser smoke cases
   (18,132 lines) as the behaviour record for Classic.

---

## 2. Words

| Word | Meaning here |
|---|---|
| **Workbook** | One open Excel workbook. Its content (cells, names, formats, objects, settings) is the **model**. |
| **View part** | One custom XML part in the workbook (`urn:dnacalc:views:v1`) holding everything DNA Calc adds: anchors, declarations, view preferences, embedded skins, notes and findings, the agent trail, saved view sessions. Excel keeps it without reading it (to be shown across save routes, dc-1nbp.22). |
| **Presentation** | Content of the view part. A presentation edit changes the view part and nothing else. |
| **Skin** | A manifest (data): root component, regions, panels, tokens, keymap layer, command placements, declaration schema, required host level. Built-in skins: `classic`, `inspector`, `notebook`, `map`, `app`, `forms`, `explorer`. |
| **View** | A skin mounted in a frame over one workbook (or, for skin 0, over one formula). It has a `ViewId` and its own session state (scroll, camera, folds). |
| **Declaration** | A skin's per-workbook data in the view part: a notebook's cells, a map's layout, an App's pages, an Inspector bench. It refers to model content only through anchors. |
| **Anchor** | A view-part identity for a piece of the model (a cell, range, spill, name, table or object), with a fingerprint, reconciled by the host after edits here and in Excel. Never a hidden name (ruling 13). |
| **Component** | Rust code that renders data and turns gestures into commands. Built in (`dna-ui`) or an installed signed extension. |
| **Frame** | One window: a primary view, at most one companion view, docked panels, the shell around them. One selection per frame. |
| **Forms maker** | The view that authors App pages (and reader-style write-ups, ruling 6). It shows the same live page the App view runs, with handles, a palette and a property panel; it is a view, not a mode of the App view. |
| **Level 0 / level 1** | The formula host and the workbook host (section 5). |

The code uses "skin" today for the six current views (`apps/dna-calc/src/skin.rs:1-6`). The new
meaning covers that use: a built-in view is a skin with a root component.

---

## 3. Layers and crates

```
 page (wasm, the same bundle in the browser and in WebView2)
 ┌──────────────────────────────────────────────────────────────────────────────┐
 │ apps: dna-calc (web entry) · dna-onecalc (Formula Explorer web entry)        │
 │ dna-shell        frame, Mast/Deck/Strip, command search, file, print,        │
 │                  history, macro bar, settings, studio, view switcher         │
 │ dna-skin-classic · -inspector · -notebook · -map · -app · -forms · -explorer │
 │ dna-ui           Leptos components on Strand tokens (grid surface, formula   │
 │                  field, data frame, chart, graph, panels, controls, dialogs) │
 │ dna-skin-kit     Book client, Live<T>, command bus + registry, keymap,       │
 │                  focus/input routing, view host, declarations, probe         │
 │ dna-grid-core    DOM-free grid logic (selection, edit session, geometry,     │
 │                  render plan, tile mirror, drawn shapes, controls)           │
 │ dna-strand       tokens, themes, densities, contrast law                     │
 └───────────────────────────────┬──────────────────────────────────────────────┘
                                 │ calc-skin-api::wire (JSON control, binary tiles)
 service (wasm Web Worker in the browser, native thread/process on the desktop, in-process in tests)
 ┌───────────────────────────────┴──────────────────────────────────────────────┐
 │ dna-calc-service   level-1 server: one dispatcher over ModelCommand/ViewEdit, │
 │                    query handlers, tile publication, command journal          │
 │ dna-formula-service level-0 server (used alone by the Formula Explorer)      │
 └───────────────────────────────┬──────────────────────────────────────────────┘
 engine side (crates/)           │
 ┌───────────────────────────────┴──────────────────────────────────────────────┐
 │ calc-skin-api      the contract types + wire codec (portable, no engine)      │
 │ calc-host          level 1: HostSession, history, anchors, subscriptions      │
 │ calc-formula-host  level 0: FormulaHost (trials, callables, providers, VBA    │
 │                    module UDFs, RTD topics, catalogue, conditional formats)   │
 │ calc-engine · calc-doc · calc-doc-model · calc-fml · calc-func · calc-value  │
 └──────────────────────────────────────────────────────────────────────────────┘
```

**Rules, checked in `tools/xtask/architecture.json`** (the closure-ban form its
`dependency_rules` already use for `dna-onecalc`, `dna-formula-ui` and `dna-calc`):

- `calc-skin-api` reaches no calc-fml evaluator, calc-engine, calc-doc or calc-host. It depends on
  calc-value, calc-syntax (spans), calc-doc-model (the view part's stored types, below) and serde.
- The view part's stored types (`ViewPart`, `Anchor`, `AnchorId`, `DeclId`, `ViewPrefs`) live in
  calc-doc-model, as the v1 `SkinLayout` does today (`crates/calc-doc-model/src/layout.rs`,
  812 lines): they are document metadata that `ModelRoot` holds and structural edits transform.
  Declaration bodies are opaque there. calc-skin-api re-exports them, so no model crate depends on
  a UI contract.
- `calc-formula-host` never reaches calc-engine or calc-host.
- `dna-skin-kit`, `dna-ui`, `dna-shell` and every `dna-skin-*` never reach calc-host, calc-engine,
  calc-doc or either service. They may use calc-func's number-format renderer (as the page does
  today, `apps/dna-calc/Cargo.toml` comment on `calc-func`) and calc-syntax.
- `dna-skin-explorer`, `dna-onecalc` and `dna-onecalc-desktop` never reach calc-host,
  calc-engine, `dna-calc-service` or any workbook skin.
- calc-host stops depending on an app crate. Today it uses `dna_calc_model::printing::paginate`
  and two projection types (`crates/calc-host/src/print_plan.rs:9-10`, through
  `crates/calc-host/Cargo.toml:28`); pagination moves into calc-host's print plan, and the page
  reads the plan as a `PrintPlan` answer.

---

## 4. The workbook contract (`calc-skin-api`)

The contract is what a skin may do: what a user could do in Excel, plus arrange its own
presentation. It holds no engine handle. Every type below is `Clone + Debug + Serialize +
Deserialize`, so the same value travels from a key press, a menu, command search, a macro, an
add-in or an agent.

### 4.1 Targets and the address grammar

One grammar for what a person types, a macro passes, a skin binds and an agent sends (Strand
tenet 7). A view-part anchor is a target too, so declarations never store addresses that go stale.

```rust
/// A sheet's document-stable key (xlsx sheetId, calc_doc_model::SheetKey); sessions map it to SheetId.
pub use calc_value::{CellKey, GridArea, SheetId};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceRef {
    Cells { sheet: SheetId, areas: Vec<GridArea> },     // Monthly!B4, Inputs!C5:D16, multi-area
    Spill { anchor: CellKey },                           // Report!H4#
    Name { scope: NameScope, name: String },             // ServicesShare, Sheet1!Local
    Table { name: String, part: TablePart },             // Orders, Orders[Amount], Orders[#Headers]
    Object { sheet: SheetId, name: String },             // Loan!"Balance"
    Sheet(SheetId),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    Source(SourceRef),
    Anchor(AnchorId),               // a view-part identity, resolved by the host
    Item(ViewItemRef),              // a presentation item: a card, paragraph, box, port, page element
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AnchorId(NonZeroU128);   // written as a UUID, as BlockId is today (calc-doc-model layout.rs)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeclId(NonZeroU128);
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewItemRef { pub decl: DeclId, pub item: String }

impl SourceRef {
    /// Excel syntax in, typed target out; A1 or R1C1 by the workbook's setting. A name that
    /// resolves in two scopes is refused, never guessed (R-6).
    pub fn parse(text: &str, book: &BookInfo, style: RefStyle) -> Result<SourceRef, AddressError>;
    pub fn display(&self, book: &BookInfo, style: RefStyle) -> String;
}
```

The deep-link grammar for the website (section 14) and the agent tools (section 12) is this
grammar plus a skin id.

### 4.2 One selection

Excel has one selection per window, and VBA reads it (`Selection`, `ActiveCell`). The host keeps it
(`HostSession`, `crates/calc-host/src/session.rs:443`; `HostSession::selection`, `:737`) and the
page keeps an optimistic copy so a key press moves the cursor at once. Each sheet's last selection,
scroll position and zoom are that sheet's own sheet-view data, saved in the workbook as Excel saves
them.

```rust
pub struct BookSelection {
    pub seq: SelectionSeq,               // host order; the page's optimistic moves carry a page seq
    pub sheet: SheetId,
    pub areas: Vec<GridArea>,            // Excel's areas, with the active cell inside one of them
    pub active: CellKey,
    pub object: Option<ObjectRef>,       // a selected chart or shape replaces the cell selection
    pub grouped: Vec<SheetId>,           // grouped sheets (A-18, A-114)
    pub item: Option<ViewItemRef>,       // the presentation item it was made on, if any (Pro 19 §3)
}
```

- A view draws the selection on the finest item that contains it; when the item is coarser the
  Strip says so. A source absent from the destination view gives "Reveal in Classic / Add to this
  view", never a substitute selection (R-3).
- Selection moves are navigation: never undo steps, but each undo step restores the selection it
  was made in, as calc-host already does (`history.rs` `Step::before/after`).
- Every command carries the selection sequence it was resolved against. The service refuses a
  command resolved against a stale selection, as it does today (`server.rs:690`), so optimistic
  moves and commands stay in order.

### 4.3 Commands: one vocabulary, four kinds

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// Changes workbook content as an Excel operation would. Undoable, attributed, respects
    /// protection and read-only opening.
    Model(ModelCommand),
    /// Changes the view part only. Undoable in the same history; the model fingerprint is unchanged.
    Presentation(ViewEdit),
    /// Calculates or sets calculation settings. Not undoable, as in Excel.
    Calculate(CalcCommand),
    /// Moves the selection, reveals, switches view. Back/Forward, not undo.
    Navigate(NavCommand),
}
```

`ModelCommand` is the union of today's 67 page operations (`apps/dna-calc/src/workbook_command.rs:35-499`)
and calc-host's `SkinIntent` (`session.rs:145-244`), deduplicated and grouped by Excel area. Its
shape:

```rust
pub enum ModelCommand {
    // Entry: Excel's entry rules (calc_host::interpret_entry), one transaction each.
    Enter { at: CellKey, text: String },
    EnterAcross { areas: Vec<GridArea>, active: CellKey, text: String, array: ArrayEntry }, // Ctrl+Enter, Ctrl+Shift+Enter
    EnterMany(Vec<(GridArea, String)>),                    // Range.Formula put
    Clear { areas: Vec<GridArea>, what: ClearWhat },
    // Clipboard, fill, moves.
    Paste { from: ClipSource, to: Vec<GridArea>, special: PasteSpecial },
    Fill { from: GridArea, to: GridArea, kind: FillKind },
    Move { from: GridArea, to: CellKey, copy: bool, insert: bool },
    FlashFill { area: GridArea },
    // Structure and layout of the sheet.
    Shift(ShiftEdit), Axis(AxisEdit), Sheet(SheetEdit), Merge(MergeEdit), Panes(PaneEdit),
    // Formats, rules, names, data.
    Format(FormatEdit), Conditional(ConditionalEdit), Validation(ValidationEdit),
    Name(NameEdit), Table(TableEdit), Filter(FilterEdit), Sort(SortEdit), Outline(OutlineEdit),
    Pivot(PivotEdit), Scenario(ScenarioEdit),
    // Objects.
    Chart(ChartEdit), Shape(ShapeEdit), Sparkline(SparklineEdit), Control(ControlEdit),
    // Excel's own notes, comments, hyperlinks (model content, unlike DNA Calc review notes).
    Note(CellNoteEdit), Hyperlink(HyperlinkEdit),
    // A formula edited by structure: replace one reference occurrence, rename a name everywhere.
    // The result is exact Excel text; the map's wiring and "Rewrite as LET" use it (R-30, R-5).
    Rewrite(FormulaRewrite),
    PageSetup { sheet: SheetId, change: PageSetupChange },
    // History.
    Undo, Redo, Revert { step: StepId },
}

pub enum CalcCommand { Now, Sheet(SheetId), Full, Rebuild, Areas(Vec<GridArea>),
                       SetMode(CalcMode), SetIteration(Iteration), SetFullPrecision, Cancel }

pub enum NavCommand { Select(BookSelection), GoTo(SourceRef), Reveal { target: Target, view: Option<SkinKey> },
                      SwitchView { sheet: Option<SheetId>, skin: SkinKey }, Back, Forward }
```

The shell has a fifth, local kind, `AppCommand` (open, save, print, settings, macro security,
add-ins, panels, theme, try-on). It never crosses the workbook wire, but it is registered in the
same command registry (section 6.3), so keys, key tips, menus, command search and agents reach it
the same way.

**The object model is not a second command set.** VBA, COM and Office.js keep Excel's object
model (`crates/calc-object-model`, with the workbook members in `calc_host::automation`), which is
far wider than what a skin can reach and is Excel's own interface. Its writes are host transactions
like any other: `Origin::Macro` or `Origin::Automation`, recorded in the one history, announced by
the same `Committed` event, and refused by the same protection and read-only rules. A skin never
calls the object model, and the object model never goes through `ModelCommand`.

**The clipboard carries the thing** (Fable §3, R-19, R-20, C-28). Copy puts the source on the
clipboard: cells with their origin (so relative references adjust as Excel's do), plus text, TSV
and HTML for other programs, and DNA Calc's own MIME entry naming the workbook, the areas and,
from a non-grid view, the presentation item. `ClipSource` is either that entry (a copy inside DNA
Calc) or foreign data (text, HTML, an image). A paste that would create model content from a view
item shows the consequence preview first (4.5). The Classic clipboard work of dc-971p (system
clipboard, Paste Special) is kept as the service's paste route (`PreparePaste`, `ApplyPaste`
today).

### 4.4 Presentation edits

```rust
pub enum ViewEdit {
    Declare { decl: DeclId, skin: SkinKey, sheet: Option<SheetKey>, body: serde_json::Value },
    /// RFC 6902 patch against the version the editor read; a concurrent edit is refused, not merged.
    Patch { decl: DeclId, expect: DeclVersion, patch: JsonPatch },
    Remove { decl: DeclId },
    Anchor(AnchorEdit),            // create, rebind after review, drop
    Prefs(ViewPrefs),              // preferred and supported views, the view of each sheet
    Review(ReviewEdit),            // DNA Calc review notes and findings (ruling 5)
    Skin(SkinEdit),                // embed or remove a skin manifest in the file
    Trail(TrailSetting),           // the agent trail on or off (ruling 14)
}
```

The host stores declaration bodies as opaque, versioned JSON. It checks size, version and that
every anchor a body lists exists; the skin checks the body against its own schema. So a new skin,
an extension or an agent can store presentation without a host change, and the host still keeps
every anchor correct (section 10).

**The route is today's, with new content.** A `ViewEdit` is a workbook operation, as
`SkinLayoutEdit` is now: calc-host turns `CellEdit::Layout` into `WorkbookOp::EditSkinLayout`
(`crates/calc-host/src/command.rs:202`) and its history records the inverse
(`crates/calc-host/src/history.rs:123,169`). The new `WorkbookOp::EditView(ViewEdit)` takes that
place, so presentation edits get the revision, the history, the save and the dirty flag with no
second mechanism. The model fingerprint is computed over everything except document metadata,
which is how "presentation only" is checked.

**Continuous gestures.** Dragging a box, resizing a panel or scrubbing a slider changes the view's
session state while the pointer is down; release commits one `ViewEdit` (one undo step), or one
model transaction for a slider bound to a cell. Nothing is written per pointer move.

### 4.5 Transactions, origin and outcomes

```rust
pub struct Transaction {
    pub label: Label,                  // "Rename output to pending"
    pub origin: Origin,
    pub expect: Option<Revision>,      // refuse if the workbook moved on (stale Explorer apply, R-38; agent base, R-47)
    pub selection: SelectionSeq,       // the selection it was resolved against
    pub units: Vec<Unit>,              // all committed or none; each unit is its own undo step, in order
}
pub struct Unit { pub label: Label, pub commands: Vec<Command> }   // Model and Presentation only

pub enum Origin {
    User { skin: Option<SkinKey> },
    Macro { run: RunId, procedure: String },
    Automation,                         // COM, Office.js
    Addin { provider: ProviderId },
    Agent { agent: AgentId, proposal: ProposalId },
    Extension { id: ExtensionId },
}

pub struct Outcome {
    pub txn: TxnId,
    pub revision: Revision,             // authored revision after the commit
    pub steps: Vec<StepId>,
    pub kind: ChangeKind,               // Model | Presentation | Both
    pub touched: Touched,               // areas, names, objects, declarations, anchors
    pub selection: Option<BookSelection>,
}

pub enum Refusal {
    ReadOnly,                                   // the file was opened read-only (ruling 6)
    Stale { current: Revision },
    StaleSelection,
    Protected { target: SourceRef },
    Invalid { reason: String },                 // e.g. a partial array edit (A-51)
    Validation { rule: ValidationSummary },     // Stop-style data validation (A-118)
    NotModelled { owner: &'static str, what: String }, // typed refusal naming the owning bead
    Irreversible { at: CellKey },               // would remove what no edit restores
}
```

Two units in one transaction give R-46: the agent's A+B acceptance is atomic, and Ctrl+Z then
undoes only B.

**Preview.** Every non-grid editor shows its consequence before commit (the design sources call
this the seam): which cells, names and anchors change, which references adjust, which spill is
blocked, which formulas would show `#NAME?`.

```rust
pub enum PreviewAnswer { Ready(Preview), Declined { reason: String, alternatives: Vec<Alternative> } }
pub struct Preview {
    pub base: Revision,
    pub kind: ChangeKind,
    pub edits: Vec<CellChange>,             // exact old and new inputs
    pub references: Vec<RefAdjustment>,     // formulas whose text changes and how
    pub spills: Vec<SpillChange>,
    pub errors_added: Vec<CellKey>,
    pub values: Option<ValuePreview>,       // recalculated values, only within budget (section 17.4)
}
```

### 4.6 One history

The host's `History` (`crates/calc-host/src/history.rs`) stays the one history and gains
presentation: a `Step` records model inverses (today) and view-part inverses (new). Its labels,
kind and origin are readable:

```rust
pub struct HistoryView { pub undo: Vec<StepInfo>, pub redo: Vec<StepInfo> }
pub struct StepInfo {
    pub id: StepId, pub label: Label, pub origin: Origin, pub kind: ChangeKind,
    pub at: Revision,
    pub external: Vec<ExternalEffect>,    // what undo cannot reverse (files written, add-in calls with effects)
}
```

- **Order of undo.** Ctrl+Z first undoes inside the focused editor's draft (`TextHistory`, kept);
  once committed, it acts on the workbook history, from any view (A-98, R-21). The undo notice
  names the step and its kind ("Undid: moved box *Stock* in Model map, presentation") with a Show
  action, so undoing a step made in another view is never silent.
- **Revert** (Fable C-41) writes a new step that restores what an older step changed, refused
  where later work touched the same targets.
- **Recovery** (C-14, R-22, R-49; Neon R-47). The shell journals committed transactions and open
  drafts apart, in local storage (OPFS in the browser, a file beside the settings on the desktop).
  After an abnormal exit it offers Restore or Discard. Restoring replays only transactions whose
  `TxnId` the reopened file does not already hold, so an accepted change never applies twice; drafts
  and unaccepted proposals come back as drafts and proposals, never applied. The service's command
  journal (`calc_host::CommandJournal`, `server.rs:395`) already deduplicates by id within a
  session; this extends the same identity across a restart.
- **Macro runs** are section 17.1.

### 4.7 Reads: queries, live values, budgets

```rust
pub trait Query: Serialize + DeserializeOwned + 'static {
    type Answer: Serialize + DeserializeOwned + Clone + PartialEq + 'static;
}

pub enum Observed<T> {
    Loading,
    Ready { value: T, at: Revision, coverage: Coverage },
    Stale { value: T, at: Revision, reason: StaleReason },        // shown with its reason, never as current
    Declined { reason: String, alternatives: Vec<Alternative> },  // over budget: offer a smaller scope
    Failed(String),
}
pub enum Coverage { Complete, Partial { done: u64, total: Option<u64>, note: String } }
pub struct Budget { pub cells: u64, pub nodes: u32, pub edges: u32, pub bytes: u64, pub millis: u32 }
```

The query vocabulary is neutral: no query is named after a skin. That removes every per-skin
projection from the service (brief item 3).

| Query | Answer | Used by |
|---|---|---|
| `Book` | name, sheets (key, name, kind, visibility), access mode, file format, date system, macro presence, platform capabilities | all |
| `CalcStatus` | mode, iteration, state, authored and calculated revision, dirty count, cycles, live topics | Strip, all |
| `Cell(CellKey)` | input, value, shown text, format, provenance, spill/array membership, editability, validation, note | formula bar, Inspector, Explorer |
| `Names`, `Tables`, `Objects`, `ChartData` | inventories with source ranges and series values | Classic, Notebook, App, Map |
| `Dependencies { of, direction, depth, group, budget }` | nodes and edges grouped by sheet, block or name; dynamic and unresolved references marked; counts and exceptions | Inspector, Map |
| `Trace { cell, budget }` | evaluation tree with sub-expression values (exists: `TraceFormula`) | Inspector, live tree |
| `EvaluatePart { cell, span }` | value of a sub-expression (F9 in the editor, A-46) | editor |
| `Regions { sheet, budget }` | recognised structure with evidence: tables, blocks, runs, period axes, inputs | Inspector, Map, Notebook, forms maker |
| `Rows { source, filter, window }` | a bounded, filtered row window with exact totals | data frame, Inspector |
| `Find`, `Stats`, `CanEdit`, `PrintPlan`, `Inventory` | as named | shell, Classic |
| `Live { topic }` | observed RTD and feed updates with source and receipt times, coalescing counts | Inspector, Notebook, App |
| `ViewPart` | anchors with their resolution, preferences, declarations | kit |
| `Formula(FormulaQuery)` | level-0 queries (section 5) | formula field, Explorer |

Cells use the existing tile publication, because it is the one read that must scale with the
viewport: one flight per subscription, coalescing, barrier acknowledgement
(`apps/dna-calc-worker/src/publication.rs`), committed mirrors (`apps/dna-calc-model/src/tile_mirror.rs`).
Interests gain one kind, so notebooks and App pages subscribe to what they bind:

```rust
pub enum Interest { Area(GridArea), Areas(Vec<GridArea>), Sheet(SheetId), Selection,
                    Anchors(Vec<AnchorId>) }   // new: the host resolves anchors, spills to their extent
```

Other queries are kept live by invalidation: each answer carries the footprint it read (areas,
names, declarations); after a commit or a calculation the service sends `Invalidated { queries }`
and the kit asks again only for queries a mounted view still observes.

### 4.8 Calculation state and evidence

```rust
pub enum Provenance {
    Calculated { revision: Revision },
    Saved,                                       // the file's cache, not recalculated here
    Pending,
    Stale { since: Revision, cause: StaleCause },// manual mode, an input changed
    External { source: SourceId, observed_at: Option<Timestamp> },
    Refused { owner: String, reason: String },   // a typed not-modelled refusal, counted as a failure
}

pub struct CalcStatus {
    pub mode: CalcMode, pub iteration: Iteration, pub precision_as_displayed: bool,
    pub state: CalcState,                 // Idle | Running { done, total } | Waiting { on } | Cancelled | Failed
    pub authored: Revision, pub calculated: Option<Revision>,
    pub dirty: u64, pub cycles: Vec<Vec<CellKey>>, pub last: Option<Duration>, pub live_topics: u32,
}
```

- A formula error stays its Excel value; a missing provider or an unfinished calculation is a
  provenance or a status, never an invented error code (R-9).
- A PivotTable's cache has its own refresh state (when it was refreshed, against which source
  revision), separate from formula calculation, so a stale pivot is never shown as proof of a fresh
  source total (R-25).
- Evidence is offered only when observed. `Dependencies` is a dependency explanation;
  `CalcChain` answers `NotObserved` unless the host recorded an actual order (Pro 19 push-back 2,
  R-10).
- **Consistent reads.** Print, App outputs and agent previews read through
  `book.consistent(|reads| ...)`, which yields only when every read is at one calculated revision
  and otherwise reports Pending. No mixed-revision page is labelled current (R-8, C-29).

### 4.9 Events

Views mostly read `Live` values; events are for what is not a value.

```rust
pub enum BookEvent {
    Committed { txn: TxnId, revision: Revision, origin: Origin, kind: ChangeKind, label: Label, touched: Touched },
    Calculated { revision: Revision, tick: CalcTick, changed: ChangedSummary, outcome: CalcOutcome },
    Progress { phase: Phase, done: u64, total: Option<u64> },
    SelectionMoved { selection: BookSelection, origin: Origin },
    History { undo: Option<Label>, redo: Option<Label> },
    MacroRun { run: RunId, procedure: String, phase: RunPhase },        // Started | Ended { effects }
    Live { topic: TopicId, value_at: Option<Timestamp>, received_at: Timestamp, coalesced: u32 },
    Providers { changed: Vec<ProviderId> },
    ViewRequest { origin: Origin, request: ViewRequest },  // from the views library for VBA: switch, select, show panel
    Saved { format: FileFormat }, SaveFailed { reason: String },
    Refused { txn: TxnId, refusal: Refusal },
}
```

**The views library for VBA** (G02; Fable R-99; R-65). A macro asks for presentation through
`Application.Run`: `Application.Run "DnaCalc.View", "Notebook"`, `"DnaCalc.Select", "Loan!B2:B6"`,
`"DnaCalc.Panel", "inspector.trace", True`. calc-host's VBA bridge turns each into a
`BookEvent::ViewRequest`. A selection moves at once, because the host holds it and `Selection`
must read it back; a view switch or a panel is carried out by the shell when the run ends or
yields, never during a user's edit (Fable's rule). None is an undo step. In Excel the same line raises a trappable "cannot
run the macro" error, so `On Error Resume Next` makes it harmless and calculation never depends on
it. A macro cannot dismiss a trust card, hide the Strip or change a skin. No type library or
reference is added to the workbook.

### 4.10 What the contract does not give

No engine handle, no file bytes, no way to set a value except through an Excel operation, no
hidden calculation, no per-skin service code. A skin cannot change how anything calculates; the
engine still does no more than Excel.

---

## 5. Host levels

### 5.1 Level 0: the formula host (`calc-formula-host`)

The brief's first decision (item 9) is option (b): level 0 is its own crate below calc-host. A
crate boundary is checked by architecture.json; a cargo feature unifies across the workspace and
erodes. Level-0 session state (registered UDFs, RTD topics, options) lives in one object both
levels use, so a skin handles one session object per level and calc-host owns a level-0 object
instead of copying its services.

Level 0 already exists as code: the Formula Explorer's `Session`
(`apps/dna-onecalc/src/session.rs:193`, 1,386 lines) evaluates a formula in a small fixture of
input cells through calc-fml, with a callable table, a VBA module or project, macro policy, add-in
references, RTD admission, a calculation environment and the result's look (cell style and
conditional formats). It "holds no dependency graph and no recalculation engine" (its own doc
comment). It moves, with `cf.rs`, `rtd.rs`, `trace.rs` and `fixture.rs`, into the new crate and
becomes the one level-0 object:

```rust
pub struct FormulaHost {
    callables: calc_fml::CallableTable,   // built-ins + published UDFs (XLL, COM, JS) + VBA module
    providers: ProviderRegistry,          // who registered what, version, trust, execution host
    rtd: RtdTopics,                       // admission (moves from calc-host recalc.rs:452-560)
    environment: Option<CalcEnvironment>, // date system, locale, clock; None refuses date/format functions
    macro_policy: MacroPolicy,            // Off | Guarded | Unguarded, for a VBA module's UDFs
    plans: PlanCache, scratch: EvalScratch,
}

impl FormulaHost {
    pub fn evaluate(&mut self, trial: &Trial) -> TrialResult;            // value, shown text, diagnostics
    pub fn evaluate_part(&mut self, trial: &Trial, span: Span) -> Value;  // F9 on a selection (A-46)
    pub fn trace(&mut self, trial: &Trial) -> TraceTree;                 // the live tree
    pub fn conditional(&mut self, look: &Look, result: &Value) -> CfOutcome;
    pub fn functions(&self) -> FunctionCatalogue;                         // descriptors, provider, refusal status, version
    pub fn help(&self, name: &str) -> Option<FunctionHelp>;               // calc_catalogue::HELP
    pub fn publish(&mut self, table: calc_fml::CallableTable);            // level 1 or an add-in load sets this
    pub fn load_vba(&mut self, module: VbaSource) -> Result<ProjectReport, String>;
    pub fn admit_rtd(&mut self, call: &RtdCall) -> RtdAdmission;
}

pub struct Trial {
    pub formula: String,
    pub at: CellKey,                        // the formula cell in the trial's own sheets (A1 by default)
    pub form: FormulaForm,                  // dynamic-array or legacy, as entered
    pub inputs: Vec<(CellKey, InputValue)>, // input cells by address, as the Explorer's input table has today
    pub sheets: Vec<String>,                // the trial's sheet names
    pub caller: Option<CallerSnapshot>,     // a real cell's context, passed down by level 1
}
pub struct CallerSnapshot { pub book: String, pub at: String, pub revision: Revision,
                            pub precedents: Vec<(String, Value)>, pub format: Option<String> }
```

A caller-dependent function with no caller is refused with a route to a real workbook cell (R-39).
A provider that cannot be safely previewed does not run on each keystroke (R-42); its function
info says so.

**What runs on the page, not in level 0.** Completion and signature help run on the page, in the
formula field, over the `FunctionCatalogue` the service sends once and again whenever its version
changes (`BookEvent::Providers`), plus the workbook's names. A keystroke never waits for a round
trip. Number formatting is one pure function, `calc_func::cell_text_localized`
(`crates/calc-func/src/format/mod.rs:1786`), which the page already calls for cell text; the
Explorer's formatting playground calls the same function. Only conditional formats, which evaluate
formulas, go to level 0.

What moves out of calc-host into level 0: RTD admission (`set_rtd_connector`, `admit_connected_rtd`,
`update_rtd`, `retire_rtd`, `crates/calc-host/src/recalc.rs:452-560`, already built on
`calc_fml::rtd::admit`) and callable registration bookkeeping (`state.rs:289`, `session.rs:489`).
What stays in calc-host: the engine bindings (`engine.set_callable_bindings`,
`engine.invalidate_rtd_topic`), workbook conditional formats over references, the workbook's VBA
project, and everything with a revision. The Explorer's evaluation path
(`Session::calculate*`, `session.rs:1022-1204`) becomes `FormulaHost::evaluate`, so DNA Calc's
formula bar (F9 on a part, the live tree) and the Explorer use one route.

### 5.2 Level 1: the workbook host

calc-host keeps its role (`crates/calc-host/CONTRACT.md`): authored revisions, transactions,
history, recalculation, subscriptions, save. Changes:

- `HostState` owns a `FormulaHost` and delegates to it; it never copies a level-0 service. The
  engine's callable bindings and RTD invalidation are fed from it.
- `dispatch(Command)` replaces `SkinIntent` (`session.rs:145-242`); `ModelCommand` variants map
  onto the existing command functions directly.
- The view part replaces `SkinLayout` as `ModelRoot`'s document metadata. Its anchors are
  transformed by structural edits as references are (section 10), and `WorkbookOp::EditView`
  carries its edits through the existing history.
- `preview(&Transaction, Budget)` is new (section 17.4).
- Macro runs get a run scope (section 17.1).

### 5.3 The service and the wire

The service holds a host behind the wire and is the same code in three places: a Web Worker
(browser), the desktop process (WebView2), and in-process (tests, `dnacalc serve`). It keeps what
works in `apps/dna-calc-worker` today: the request loop, the command journal with retry and
deduplication (`server.rs:395`), sliced calculation with a budget (`worker.rs:96,123`), tile
publication with acknowledgement, and the shared message decoding for browser and desktop
(`message.rs`). It loses the per-skin builders and the intent translation.

```rust
pub enum ToService {
    Command { id: u64, txn: Transaction },
    Query { id: QueryId, query: QueryRequest, budget: Budget },
    Forget(QueryId),
    Preview { id: u64, txn: Transaction, budget: Budget },
    Subscribe { id: u64, interest: Interest }, SetInterest { id: u64, interest: Interest }, Unsubscribe(u64),
    TileAck(TileBarrier), ResetTiles(u64),
    Formula { id: u64, request: FormulaRequest },          // level 0
    Session(SessionRequest),                               // open, save, macro security, add-ins
}
pub enum FromService {
    Outcome { id: u64, result: Result<Outcome, Refusal> },
    Answer { id: QueryId, answer: Observed<serde_json::Value> },
    Invalidated(Vec<QueryId>),
    Tiles(TileEvent),                                      // admission, fragments, completion (binary)
    Event(BookEvent),
    Formula { id: u64, reply: FormulaReply },
    Failed { id: u64, message: String },
}
```

A level-0 service (`dna-formula-service`) answers `Formula` and `Session` requests and refuses the
rest with a typed refusal. The level-1 service answers `Formula` through its host's `FormulaHost`,
with the same handler code.

### 5.4 Which level a skin needs, and the Explorer inside DNA Calc

Each manifest names its level (`"level": "formula" | "workbook"`). The view host mounts a workbook
skin only over an open workbook. Skin 0 runs with `book: None` standalone, or with a book and a
`CallerSnapshot` inside DNA Calc: "Explore this formula" on Summary!D14 passes the formula, the
caller, the revision and the precedents' values down. Level 0 never reads the workbook. Apply is
a level-1 transaction with `expect: Some(revision)`; if the cell changed meanwhile the Explorer
shows the conflict instead of overwriting (R-37, R-38).

---

## 6. The kit (`dna-skin-kit`) and the registry

The kit is toolkit-free apart from the reactive core Leptos itself uses (`reactive_graph`), so
every rule in it runs in native tests and in the browsers without a DOM.

### 6.1 Skins are manifests; components are registered

The current `Skin` trait (`apps/dna-calc/src/skin.rs:59`) makes a skin a piece of code with an id,
a title, a focus target and a `mount`. In the new shape a skin is only its manifest, and code
enters through one registry, so a built-in view and a user's restyled copy of it are the same kind
of thing:

```rust
pub struct Registry { /* components, commands, keymap layers, manifests */ }

impl Registry {
    /// A root or panel component, by id ("classic.workspace", "inspector.trace", "dna.grid").
    pub fn component(&mut self, id: ComponentId, spec: ComponentSpec);
    /// A command (section 6.3).
    pub fn command(&mut self, spec: CommandSpec);
    /// A built-in skin's manifest, shipped as JSON in the skin crate (include_str!).
    pub fn manifest(&mut self, manifest: SkinManifest);
}

pub struct ComponentSpec {
    pub level: Level,                                  // Formula | Workbook
    pub reads: &'static [QueryKind],                   // declared, so a probe and an extension host can check
    pub declaration: Option<DeclarationKind>,          // the schema it edits, if any
    pub mount: fn(ViewCx, &ComponentProps) -> AnyView, // Leptos, the baseline (ruling 7)
}

/// Each skin crate exposes one function; the app entry calls the ones it ships.
pub fn install(registry: &mut Registry);

#[derive(Clone)]
pub struct ViewCx {
    pub formula: FormulaClient,      // level 0, always present
    pub book: Option<Book>,          // level 1; None for the standalone Explorer
    pub view: ViewHandle,            // this view: id, declaration, session state, reveal, print
    pub ui: UiCx,                    // resolved tokens, density, motion, announcer, focus
}
```

- The DNA Calc web entry calls `dna_skin_classic::install`, `dna_skin_inspector::install` and the
  rest; the Formula Explorer entry calls only `dna_skin_explorer::install`. Which skins a build
  ships is a list of calls, and the dependency rules (section 3) check that the Explorer's list
  cannot grow a workbook skin.
- Mounting a manifest resolves its root and region components in the registry; a missing one is
  "Needs the extension *X*", never a fallback (section 11).
- A component receives `ViewCx` and its props; it holds no workbook state of its own.

### 6.2 The `Book` handle

```rust
#[derive(Clone, Copy)]
pub struct Book { /* handle */ }

impl Book {
    pub fn info(&self) -> Live<BookInfo>;
    pub fn calc(&self) -> Live<CalcStatus>;
    pub fn selection(&self) -> Signal<BookSelection>;
    pub fn history(&self) -> Live<HistoryView>;
    pub fn cells(&self, interest: Interest) -> LiveCells;         // tile-backed; set_interest() moves it
    pub fn cell(&self, at: CellKey) -> Live<CellFacts>;
    pub fn query<Q: Query>(&self, query: Q, budget: Budget) -> Live<Q::Answer>;
    pub fn consistent<R>(&self, read: impl Fn(&Reads) -> R) -> Live<R>;  // one calculated revision or Pending
    pub fn run(&self, command: impl Into<Command>) -> Pending<Result<Outcome, Refusal>>;
    pub fn transact(&self, txn: Transaction) -> Pending<Result<Outcome, Refusal>>;
    pub fn preview(&self, txn: &Transaction, budget: Budget) -> Pending<PreviewAnswer>;
    pub fn events(&self) -> EventStream<BookEvent>;
    pub fn draft(&self) -> DraftHandle;                            // the one edit session (6.5)
}

pub struct ViewHandle { /* handle */ }
impl ViewHandle {
    pub fn id(&self) -> ViewId;
    pub fn sheet(&self) -> Option<SheetId>;
    pub fn declaration<D: Declaration>(&self) -> Live<Option<D>>;
    /// Edits the declaration: the kit diffs it into one `ViewEdit::Patch`, one undo step.
    pub fn edit<D: Declaration>(&self, label: &str, edit: impl FnOnce(&mut D))
        -> Pending<Result<Outcome, Refusal>>;
    /// Camera, scroll, folds: not undoable, written to the view part's session section on save.
    pub fn session<S: SessionState>(&self) -> RwSignal<S>;
    pub fn on_reveal(&self, reveal: impl Fn(&BookSelection) -> RevealOutcome + 'static);
    pub fn on_print(&self, print: impl Fn(&PrintRequest) -> PrintJob + 'static);
}

pub trait Declaration: Serialize + DeserializeOwned + Clone + PartialEq + 'static {
    const SKIN: SkinKey;
    const SCHEMA: u32;
    fn anchors(&self) -> Vec<AnchorId>;                              // the host keeps these alive
    fn migrate(from: u32, body: serde_json::Value) -> Result<Self, String>;
}
```

A view of a few hundred lines is the target. For example the core of an App output tile:

```rust
#[component]
fn OutputTile(cx: ViewCx, anchor: AnchorId, label: String) -> impl IntoView {
    let book = cx.book.expect("App needs a workbook");
    let cells = book.cells(Interest::Anchors(vec![anchor]));
    view! { <ui::Tile label=label><ui::ValueView cell=cells.anchor(anchor) /></ui::Tile> }
}
```

`ValueView` shows the value in its number format with its provenance; the tile cannot show a stale
value as current, because a stale value arrives as `Observed::Stale` with its reason and the
component draws that state.

### 6.3 One command registry

```rust
pub struct CommandSpec {
    pub id: CommandId,                     // "edit.paste-values", "view.notebook.cell.insert-below"
    pub label: &'static str,
    pub kind: CommandKind,                 // Model | Presentation | Calculate | Navigate | App
    pub key_tip: Option<&'static str>,     // Classic: Excel's letters exactly, "H V V" (ruling 11)
    pub help: Option<&'static str>,        // site slug (section 14)
    pub args: ArgsSchema,                  // JSON schema: the agent's tool description
    pub available: fn(&CommandCx) -> Result<(), Unavailable>,
    pub build: fn(&CommandCx, Option<&serde_json::Value>) -> Result<Invocation, String>,
}
```

Keys, key tips, menus, the context menu, command search (Alt+Q), macros through the views library
and agents all invoke a `CommandId`; none has a private path. A disabled command says why (Appendix A's
context rule). Today's 14 shell commands (`apps/dna-calc-model/src/command.rs`) and 67 workbook commands
are the first entries.

### 6.4 The keymap engine and Excel's modes

```rust
pub struct KeyBinding { pub keys: KeySeq, pub when: When, pub command: CommandId,
                        pub args: Option<serde_json::Value>, pub profile: Profile }
pub enum Profile { Excel365WinEnUs, Jupyter, Dna, Custom(String) }
pub enum InputMode { Ready, Enter, Edit, Point, End, Extend, Add, KeyTips(Vec<char>),
                     Object, NotebookCommand }
```

- Resolution order: focused editor or open menu, then the focused component, then the view's
  layer, then the shell (R-16, Appendix A context rule).
- Chords match the physical key and modifiers, but yield when the layout produces a character
  (AltGr layouts make Ctrl+Alt+7 a `{`).
- Key tips are a state machine over the Deck's tree; the letters are data in the Classic manifest.
- A skin layer that changes a binding of the Excel profile is counted and labelled ("departs from
  Excel in 2 keys", Fable R-94). Protected keys cannot be rebound in any profile: Esc, Ctrl+Z,
  Ctrl+Y, F4 while editing, Ctrl+Enter, Alt+Q, and the way back to Classic (Pro 21 U03, R-61); a
  binding that collides with an existing one (Ctrl+E is Flash Fill) is refused before Apply. The
  Strip cannot be hidden.
- **View keys (ruling 12; Neon proposed the chords).** Ctrl+Alt+1 Classic, Ctrl+Alt+2 Inspector,
  Ctrl+Alt+3 Notebook, Ctrl+Alt+4 Model map. Excel binds Ctrl+Alt+5 (then Tab, to cycle floating
  shapes, A-111), so App takes no chord: the view switcher and command search (Alt+Q) reach it, as
  the ruling's fallback says. Ctrl+1 to Ctrl+5, which Carbon proposed, are Excel's (Ctrl+1 is
  Format Cells) and are not taken. On a layout where AltGr+2 or AltGr+3 types a character (German
  `²` and `³`), Ctrl+Alt+2 and Ctrl+Alt+3 type it as Excel does, and the switcher is the route.
- **Quick Access slots.** Alt+1 to Alt+9 are Excel's Quick Access Toolbar keys; Classic keeps
  them as that. Fable's nine quick slots are this toolbar, filled from the manifest.
- **Type-ahead.** Keys typed while a command that moves the target is pending are queued and
  replayed in order, as the controller does today (`Controller::input_pending`, `after_input`).

### 6.5 Focus, input routing and the one draft

The edit session is kit state (today's `EditSession`, `apps/dna-calc-model/src/state.rs:54`,
with its surface transfer, point mode and focus tickets): one target, one draft, one commit or
cancel, shared by the in-cell editor, the formula bar, a notebook cell editor and a map box. A view
switch moves the draft to an anchored editor in the new view or defers the switch; it never
commits implicitly (R-4).

### 6.6 The view host

```rust
pub struct Frame { pub book: Option<BookId>, pub primary: ViewSlot,
                   pub companion: Option<ViewSlot>, pub panels: Vec<PanelSlot> }
```

- At most one primary view and one companion (Fable: free tiling loses people; a tab per view
  hides that it is one workbook). Panels dock in any view: every Inspector tool is a panel.
- A sheet's preferred view is a badge on its tab, never applied by a click on the tab (the README's
  agreed reading of the four answers; Fable R-19 and push-back 4; Neon's push-back). The file's preferred view is applied once, when the
  file opens. The person's explicit choice wins over the file's preference (R-14), and a
  workbook-wide view (the map overview) names its scope.
- Mounting a view subscribes; unmounting releases. Each view keeps its session state while
  unmounted, so switching back restores scroll and camera.

### 6.7 The probe

`window.__dnaProbe()` returns the semantic state: selection, input mode, draft, focused surface,
visible cells (address, shown text, provenance), status readouts, history labels, open dialogs.
Conformance and smoke tests drive keys and pointer events and read the probe, never the DOM
structure, so a UI can be rewritten without rewriting its tests.

The existing smoke suite is most of the way there. Its 1,053 cases
(`apps/dna-calc/tests/smoke/cases.rs`, 18,132 lines) are typed `Case { name, actions, checks }`
values: `Action::Into("name-box", ...)`, `Action::Click("format-cells")`, `Check::PlanCell("A1",
"style.family", ...)` over the render plans the page publishes (`__dnacalcRenderPlans`). What ties
them to today's page is the element keys. In the new UI every command control carries its
`CommandId` (`data-command="format.cells"`) and every component its role, and the probe answers
the plan and state checks; a one-time rename of the keys ports the cases, which then survive every
redesign. Cases that test a removed skin are dropped with it and counted in the port's commit:
110 case names mention the notebook, ledger, atlas, tree or canvas (a name match, so the true
count is found case by case).

---

## 7. Components and tokens

### 7.1 The component library (`dna-ui`)

| Family | Components |
|---|---|
| Frame | Mast, Strip (fixed readouts), Deck (command bands with key tips), Dock, Region (named-area layout), Card, Split |
| Source | AddressChip, ValueView (value, number format, provenance), FormulaView (read-only, lens), FormulaField (highlight, completion, signature help, point mode, F4, F9 part), RefPicker |
| Data | GridSurface (Classic), DataFrame (virtual window over a spill or table, with coverage), Chart, Sparkline |
| Structure | Tree (outline and live tree), Graph (nodes, ports, wires, groups, pan, zoom, level of detail), Instrument (live readout, Phyphox style) |
| Controls | Field, Slider, Dropdown, Checkbox, Button, all bound to cells through data validation and control objects |
| Shell | CommandSearch, Menu, ContextMenu, Dialog, Announce, SheetTabs, HistoryList, TrustCard, Consequence (the seam) |
| Prose | Markdown with bound inline values (`{=pending}` as a live chip) |

Each component takes data from the kit and emits commands; none holds workbook state. A component
renders a value with its provenance and a target with its address, so honesty is built in once.

**Charts move to the page.** `calc-chart-render` depends on calc-host today and the worker sends
finished SVG (`server.rs:5025,5228`). It becomes a portable drawing crate over `ChartData`
(chart spec plus series values from the contract); the Chart component calls it. A visual change to
a chart then touches no service code.

### 7.2 Tokens

`dna-strand` keeps the palette, themes (cockpit-light, cockpit-dark, high-contrast), densities
(Working, Reading), type ramp, radii, gaps, motion and the contrast tests (`SANCTIONED_PAIRS`).
New:

```rust
pub fn resolve(base: Theme, overrides: &TokenOverrides) -> Result<ResolvedTheme, Vec<ContrastViolation>>;
impl ResolvedTheme { pub fn css_properties(&self) -> Vec<(&'static str, String)>; }   // --dna-* names
```

The shell applies the resolved theme through CSSOM, as `apps/dna-calc/src/style.rs` does (the CSP
forbids inline style). Semantic roles are fixed: red is error, amber is focus, signal orange is
external or volatile, green is fresh. A skin may change a shade within the contrast law, never a
role. Workbook formatting (red negatives, conditional formats) is workbook data and is never
themed (R-17). In a dark theme only automatic colours (the default font colour, no fill,
gridlines) follow the theme; explicit workbook colours, conditional formats, notes and error
cells keep theirs (U04).

### 7.3 Why this makes views beautiful

Skins write no CSS. Spacing, type, colour, motion and focus come from components and tokens; a view
arranges components in named regions. A **scene gallery** mounts each skin over the cast
workbooks at fixed scene ids (the Pro 21 ids: NC01, W04, U05 and the rest) through the in-process
service, so a visual review, the screenshot set (Pro 19 B.8) and the website's pictures come from
the real product.

---

## 8. The manifest

```json
{
  "schema": "dna.skin/1",
  "id": "classic",
  "version": "1.0.0",
  "title": "Classic",
  "based_on": null,
  "level": "workbook",
  "root": "classic.workspace",
  "regions": { "mast": ["shell.mast"], "deck": ["classic.deck"], "stage": ["classic.grid"],
               "right": [], "strip": ["shell.strip"] },
  "panels": [ { "component": "inspector.trace", "dock": "right", "open": false } ],
  "tokens": { "theme": "cockpit-light", "density": "working", "overrides": {} },
  "keymap": { "profile": "excel-365-win-en-us", "bindings": [] },
  "deck": [ { "tab": "Home", "tip": "H", "groups": [ { "label": "Clipboard",
             "commands": [ { "id": "edit.paste-values", "tip": "V V" } ] } ] } ],
  "commands": [],
  "declaration": null,
  "requires": { "components": ["dna.grid"], "extensions": [] }
}
```

A user or agent skin names `based_on` and only what it changes:

```json
{ "schema": "dna.skin/1", "id": "calm-review", "based_on": "classic", "version": "1.0.0",
  "tokens": { "overrides": { "grid.lines": "none", "formula.text.size": "15.5px" } },
  "panels": [ { "component": "inspector.trace", "dock": "right", "open": true, "width": 320 } ] }
```

- **Commands in a manifest** are named sequences of registered commands (Fable §7). They run
  through the ordinary path with the ordinary checks; they are data, not macros.
- **Validation before use:** schema; contrast; keymap departures counted by the validator (never
  stated by the manifest); every component
  installed; fixed rules kept (Strip readouts, Esc, the way back to Classic, trust cards, honest
  status, source identity). A failing manifest is not applied, and the reasons are given in plain
  words (R-61, R-92).
- **Try-on:** a manifest is applied as a session overlay under a band "Trying *Calm review* ·
  Keep · Edit · Discard (Esc)". Keep stores it with the user, or in the file as a presentation edit
  (one undo step). The model is unchanged and the check says so from the fingerprint (R-60, R-91).

---

## 9. The views on the contract

Every view below is a built-in skin: a manifest plus a root component, reading through `Book` and
writing through commands. The table at the end says what each one stores.

### 9.1 Classic

- **Components:** Deck (Strand command bands with Excel's tab, group and key-tip structure;
  letters exactly Excel's, ruling 11), name box, FormulaField, GridSurface, SheetTabs, Strip,
  dialogs (Format Cells, Paste Special, Find and Replace, Go To, Name Manager, conditional formats,
  validation, page setup, print preview, pivot fields, chart tools).
- **GridSurface** is the one large component. It keeps the current pure logic: viewport tiles and
  the committed mirror, `RenderPlan` as the display list (ready for a WebGPU painter later, owner's
  standing direction), the painter and its Excel paint-compare route
  (`tools/xtask-harness/src/app_paint.rs`), the 4,096-cell semantic layer for screen readers,
  frozen and split panes, and the gesture recognisers (fill handle, border drag to move, copy or
  insert, range picking while editing, resize and AutoFit).
- **Keymap:** the Excel profile, Appendix A of Pro 19 as the conformance list.
- **Declaration:** none. Classic shows the workbook as Excel does; its window state (panes, zoom)
  is the workbook's own sheet-view data.

### 9.2 Inspector

- **Root:** a bench of tools: Trace, Dependencies (graph or list), Calculation (status, cycles,
  iteration outcome), Live (RTD and feeds), Names, Issues (error roots), Inventory (sheets, hidden
  state, parts, links, macros, add-ins), Timings. Every tool is also a panel in every view.
- **Reads:** `Trace`, `Dependencies`, `CalcStatus`, `Live`, `Names`, `Inventory`, `Regions`.
- **Writes:** none directly. A fix goes through the consequence preview into a `ModelCommand`.
- **Declaration (`dna.decl.inspector/1`):** the bench layout and the watch list (anchors).

### 9.3 Notebook: exactly like Jupyter (ruling 6)

The notebook is a chain: a formula, its result directly beneath, the next formula using that
result, with prose between (owner correction, SKIN_DISCUSSION item 9). The interaction is
Jupyter's, key for key, in the notebook's own keymap profile (`jupyter`), as Classic's is Excel's.
The calculation is the workbook's: live, as in Excel and Pluto, never run-once (SKIN_DISCUSSION
item 1). There is one mode pair, Jupyter's edit and command modes, and no author or read mode.

| Jupyter | DNA Calc notebook |
|---|---|
| Code cell | A formula cell: a formula in a backing cell, its output (that cell's value or spill) drawn beneath it |
| Markdown cell | Prose in the declaration (presentation), with live bound values (`{=pendingTotal}` renders as a value chip with its source) |
| Raw cell (R) | None; R is refused with "no raw cells: use prose" |
| Enter (command mode), Esc (edit mode) | Enter edits the selected cell. Esc returns to command mode and keeps the unrun text, as Jupyter does: the cell is marked "edited, not run", and the output beneath stays the committed formula's, labelled so. Nothing reaches the workbook until the cell is run, so Esc never commits anything |
| Enter (edit mode) | A line break in the formula (Excel stores it), as in Jupyter's editor |
| Shift+Enter, Ctrl+Enter, Alt+Enter | Run: commit the formula (one model transaction) and move to the next cell, adding one at the end; commit in place; commit and insert below |
| Run all; Restart and run all; Interrupt (I, I) | Commit every held draft in one transaction (one undo unit per cell), then calculate (F9); rebuild and calculate fully (Ctrl+Alt+Shift+F9); cancel the calculation |
| A, B; D, D; M, Y | Insert above or below; delete (the preview lists cleared cells, the deleted name and every formula that would show `#NAME?`); to prose (clears the backing cell, previewed) or to formula (creates one) |
| Up/Down, K/J; Shift+Up/Down | Move the cell selection; extend it over several cells |
| Ctrl+Shift+Minus, Shift+M | Split and merge prose; for formula cells, refused where Excel cannot express the result |
| C, X, V; Z | Copy, cut, paste cells (a pasted formula cell gets a new backing cell, with Excel's relative-reference rules); undo is Ctrl+Z, the one history |
| O; L; H; F; Ctrl+S | Fold an output; line numbers; the key list; find and replace in the notebook; save |
| Outputs: text, table, plot | Scalar; data frame (a window over a spill or table with exact totals and coverage); chart (an ordinary Excel chart object over named ranges, NC01); function (a LAMBDA name, shown as its signature, NC05) |
| Execution count `[n]` | None. The output shows its calculation state: in automatic mode an output is never stale; in manual mode outputs go stale exactly as Excel's cells do. There is no out-of-order hidden state: the dependency graph decides what an output shows |
| A read-only notebook | The file opened read-only: every model and presentation command says so. There is no separate reading presentation; a reader-style write-up is an App page made in the forms maker (9.5) |

**Held drafts.** A cell edited and left with Esc keeps its text in the notebook view's session
state, not in the model and not in the file. Several cells may hold drafts, as in Jupyter; only the
focused one is the kit's live draft (6.5). Switching view keeps them and the Strip says "2 notebook
cells edited, not run" (R-4). Reopening never applies them (R-22). Ctrl+Z inside the editor and
"Revert cell" discard one.

**Two kinds of formula cell.** An *owned* cell has a backing cell the notebook placed. A
*referenced* cell is an anchor to an existing cell anywhere in the workbook (Maria's notebook over
Budget starts from Summary!B7, S03); editing it edits that cell in place, and deleting it from the
notebook removes presentation only.

**Backing layout: scalar rows and array lanes** (Pro 21's Analysis sheet). The notebook's backing
sheet is a visible sheet: a new one ("Analysis"), or an existing sheet the notebook is declared on
(W04's Summary). A new owned cell is placed by the shape its committed preview gives (4.5):

- a scalar takes the next row of the scalar zone above the array start row, with its label (the
  output's name, as text) in the cell to its left;
- an array takes the next free column lane from the array start row, with its label above; lanes
  run left to right with one spare column between them, so downward growth never collides;
- a formula whose shape the preview cannot give (declined: volatile, RTD, side-effecting add-in)
  takes a lane, which is safe for both shapes;
- on an existing sheet, new cells go beyond its used range, and the preview shows the destination
  before commit (R-29).

When a scalar starts to spill, or an array grows wider than its lane, the anchor shows Excel's
`#SPILL!`, nothing more. The notebook offers the fixes Excel allows, each one model transaction and
one undo step: move this cell to its own lane (Excel's cut and paste, which formulas and names
referring to it follow), move the obstruction (NC06 moves the note and its name to Y10), or insert
columns before the next lane. No calculation ever triggers an edit. Reading order and the Across
layout are presentation and change no calculation (R-26).

- **Named outputs** are ordinary workbook names (`scopeOrders` refers to `=Analysis!$A$10#`), so
  later cells and Excel users read them the same way, and the anchor follows the name.
- **What an Excel user sees:** an ordinary sheet of labelled formulas and spills, the names, and
  the chart objects. The prose is in the view part.
- **Declaration (`dna.decl.notebook/1`):** the backing sheet, the cells in reading order (kind,
  owned or referenced, anchor, name, output style, folds), the zone and lane table, and the layout
  (down or across).
- **The live tree** (a formula's LET bindings and arguments with current values, inline under a
  cell, NC04) is the first extension (section 11).

### 9.4 Model map

- **Reads:** `Dependencies` grouped by sheet group, then block, then cell, with counts, exceptions
  and dynamic references marked; `Regions` for blocks; at most 200 drawn nodes and 400 edges per
  viewport, with boundary ports and counted groups beyond (Pro 19 §6). "Expand all" over the
  budget is declined with smaller scopes offered (M03).
- **Writes:** wiring is `ModelCommand::Rewrite` (one reference occurrence) with the preview; an
  ambiguous aggregated edge opens operand selection (R-30). A new box is a model transaction (a
  label cell, a value cell and a name, in the next free row of its block on a visible sheet) plus a
  presentation placement. Removing a box removes presentation by default, labelled (R-31).
- **Delay and iteration** are different relationships: a prior-period reference is a delay, a
  cycle under iteration is a feedback enclosure (R-32, push-back 1). The ticked model runs on the
  RTD clock topic (ruling 3); Run, Pause and Step are commands to the clock provider, not to the
  calculation.
- **Declaration (`dna.decl.map/1`):** node positions keyed by anchor, name or group, folds, sheet
  pages, the camera's saved place.

### 9.5 App and the forms maker

Two views over one declaration: the App view runs pages, the forms maker authors them. Both mount
the same page renderer (`app.page`), so what the author sees is the running page, with inputs that
work while it is being made. There is no author mode inside the App view, which keeps the App view
a plain face for people who use a model and do not build it.

- **Declaration (`dna.app/1`):** pages of elements, each bound to anchors (the Pro 21 U05 shape):
  `Input { anchor, label, control, unit }`, `Output { anchor, label, emphasis }`,
  `Chart { object }`, `Frame { anchor, rows }`, `Text { markdown }`, `Button { command | macro }`,
  `NotebookExcerpt { cells }`, and the page's mode (live or batched apply).
- **App runtime:** an input is a draft until applied; applying is a model transaction through the
  cell's own data validation and protection (R-34 to R-36). A macro button sits behind the trust
  card under Guarded (G01-G03). Classic is always one action away (R-13).
- **Forms maker (`forms` skin):** the live page with selection handles, a palette of the standard
  elements, a binding picker that selects in a Classic companion (the one selection), and a
  property panel. Each change is one `ViewEdit` (one undo step). It edits the same declaration an
  agent writes (U05). Nothing is published from inference: an author confirms the inputs and
  outputs (R-68). Model changes the author asks for (a name for an input, a validation rule) are
  separate model transactions, previewed, never folded into the page.
- **Reader-style presentation** of a notebook (the write-up of NC07, the print of O03) is an App
  page made in the forms maker whose elements are notebook excerpts: same cells, a different
  presentation, not a copy (ruling 6).

### 9.6 The Formula Explorer (skin 0)

- **Level 0 only.** Components: FormulaField, the input table, ResultView (the value kept apart from
  its formatted text, R-41), the live tree, FunctionBrowser (catalogue by family, signature, help,
  examples, and the recorded match status that ships with the build), the formatting playground,
  the add-in library (providers, functions, what this host can and cannot run, R-40).
- **Standalone** (`dna-onecalc`, `dna-onecalc-desktop`): the Explorer skin, a light shell and the
  level-0 service; no calc-engine in the closure. Its session file stays the constrained xlsx it
  is today (`apps/dna-onecalc/src/file.rs`).
- **Inside DNA Calc:** a tool opened from the shell or from a cell, docked as a companion, with a
  caller snapshot (5.4).
- **The Pro 21 Explorer scenes:** E01 (function browser and live tree), E02 (array formatting
  playground: values beside their formatted text, per locale), E03 (add-in library and the limits
  of a workbook-free host) are three panels of this skin over `FormulaHost::functions`, `help`,
  `trace`, `cell_text_localized` and the provider registry.
- **Early, as a teaser** (ruling 2): it is the first skin built on the new interface (section 16).

### 9.7 What each view stores

| View | Model edits it makes | Presentation it stores (declaration) |
|---|---|---|
| Classic | every Excel operation | none (sheet views, panes and zoom are Excel's own sheet-view data) |
| Inspector | fixes, through the preview | bench layout, watch list |
| Notebook | backing formulas, label cells, names, charts, relocations and column inserts | cells in reading order, prose, zones and lanes, layout (held drafts are session state, never stored) |
| Model map | rewired references, new boxes (cells and names) | positions, folds, sheet pages |
| App | applied inputs, saved scenarios | none of its own: it runs the forms maker's pages |
| Forms maker | names or validation the author asks for, as separate previewed transactions | pages, elements and bindings |
| Formula Explorer | Apply to the caller cell only | none (its own session file standalone) |

**Mixed views per sheet** (W04: Inputs in Classic, Monthly on the map, Summary as a notebook): the
view part's preferences map each sheet to a preferred view and its declaration; the tab carries the
badge (6.6). Every view opens every workbook (FRONT_END_SCOPE §3); a view with no declaration for
a sheet opens plain and says so: the notebook offers a starting point from the selection (S03), the
map draws a layout from the dependency readings marked "drafted, not saved", the App says "No App
defined" (B.8, Inherited). A file whose view part is missing or unreadable opens in Classic with
the model intact (R-62, Fable R-106).

---

## 10. Persistence: the view part

### 10.1 The part

One custom XML part, namespace `urn:dnacalc:views:v1` (the name Pro 21 proposed), with a free part
name the writer allocates. calc-doc already has the generic route: it finds an item by its
document element's namespace and writes one as opaque bytes with its item properties, relationship
and content type (`CustomXmlItem`, `DocumentSnapshot::with_custom_xml`,
`LoadedWorkbook::custom_xml`; `crates/calc-doc/src/custom_xml.rs`). The view part goes through
that route. Its own reader and writer (in calc-doc, beside the codec) keep unknown elements and
attributes, as the v1 writer does today (`crates/calc-doc/src/save/manifest.rs:1-23`), so a newer
DNA Calc's additions survive an older one.

```xml
<dnaViews xmlns="urn:dnacalc:views:v1" version="1" writer="DNA Calc 0.x" model="sha256:…">
  <anchors>
    <anchor id="0f1e…" kind="spill" sheet="7" ref="Analysis!A10" name="pending">
      <fingerprint formula="r1c1:9b2c…" label="pending" rows="4147" cols="7"/>
    </anchor>
  </anchors>
  <prefs preferred="notebook" supported="classic notebook app">
    <sheet key="7" decl="5a2d…"/>
  </prefs>
  <decls>
    <decl id="5a2d…" skin="notebook" schema="1" sheet="7"><![CDATA[{ … }]]></decl>
  </decls>
  <skins><skin id="calm-review"><![CDATA[{ "schema": "dna.skin/1", … }]]></skin></skins>
  <review><!-- notes and findings, anchored (ruling 5) --></review>
  <trail enabled="true"><!-- agent change records (ruling 14) --></trail>
  <session><!-- per view and sheet: scroll, camera, last view; not undoable --></session>
</dnaViews>
```

- Declarations and skins are JSON inside the XML: the same serde types the kit uses and an agent
  writes, without a second schema language.
- Review notes, findings and the agent trail share the part, as rulings 5 and 14 say. The trail
  can be turned off and is capped; dropped records leave a count. A visible-sheet mirror of notes
  is optional and later (ruling 5).
- The `model` attribute is a fingerprint of sheets, names and formulas at save. On open, a
  different fingerprint means the file was edited elsewhere, and reconciliation runs (10.3).
- The `session` section (each view's scroll, camera and folds, and the last view per sheet) is
  written when the file is saved for any other reason and never makes the workbook dirty, as
  Excel treats its own sheet-view state. Held notebook drafts are never written.
- A part that this version cannot read (a newer major version, invalid content) is kept byte for
  byte, reported once ("view settings from a newer DNA Calc are kept but not shown"), and the
  workbook opens in Classic (C-27).
- Only our own test fixtures hold v1 parts (`crates/calc-doc/tests/fixtures/cdoc_m2_1`; `git grep`
  finds the namespace nowhere else but code, logs and plans), so v1 is deleted, not migrated
  (10.2).

### 10.2 Anchors in the host

The anchor table is host state, held in `ModelRoot` with the rest of the view part. Structural edits
made in DNA Calc transform anchors with the same structural transform that adjusts references
(`calc_fml::reference::StructuralEdit`, `crates/calc-fml/src/reference/transform.rs:51`); a cut
moves the anchors of the moved cells; deleting anchored cells leaves the anchor unresolved, shown
as broken until rebound. Anchors no declaration lists are dropped at save.

This replaces the v1 manifest (`urn:dnacalc:view-manifest`) and its hidden `_dna.block.*` locator
names (`crates/calc-doc/src/load/manifest.rs:10-26`), as ruling 13 requires.

### 10.3 Reconciliation after Excel

Excel keeps a custom XML part it does not understand but does not move our addresses when a user
inserts rows. On open, each anchor is resolved in this order and never guessed:

1. **Named:** the user's own name exists: bound to the name's current reference (Excel maintained it).
2. **Addressed:** the address still holds content matching the fingerprint: bound.
3. **Found:** exactly one cell or range in the sheet's used range matches the fingerprint: "moved,
   confirm" (one click rebinds; a presentation edit).
4. Otherwise **unresolved**: shown broken with Rebind and Remove; the declaration is kept (R-62).

That is why the notebook and the map anchor named outputs by name: Excel edits keep them exact.

### 10.4 The round trip with Excel

The promise is tested, not assumed (Pro 19 push-back 4): save from DNA Calc, open, edit and save in
Excel, reopen; the part is present and the anchors reconcile. The Excel route is dc-1nbp.22
(open). Document Inspector can remove the part; the workbook then opens in Classic with nothing
lost from the model.

---

## 11. The extension boundary

- **Files carry data only.** A manifest, declaration or App page names components by id. An
  unknown component shows "Needs the extension *X*, not installed"; nothing is fetched, nothing
  runs (Strand rule, README agreement, R-50).
- **Extensions are installed packages**, `name.dnaext`: `extension.json` (id, version, publisher,
  components, requested capabilities, level), `module.wasm`, and an Ed25519 signature over both.
  The publisher key chains to a root shipped with DNA Calc or added by the user. Installing shows a
  trust card like an add-in's.
- **A sandbox in a worker.** Each extension's module runs in its own Web Worker (the same in the
  browser and in WebView2), with no DOM and no network. It receives the data of the queries it
  declared (the kit proxies them and checks the capability) and returns a render tree in a
  restricted vocabulary (the `dna-ui` components, text, SVG paths, a canvas draw list) plus command
  invocations, which take the ordinary path with `Origin::Extension`. A slow or failing extension
  cannot freeze the frame.

```rust
// dna-ext-sdk (guest side, wasm32-unknown-unknown, Rust)
pub trait Component {
    fn init(manifest: &ComponentProps) -> Self;
    fn update(&mut self, data: &DataUpdate) -> Option<RenderTree>;   // query answers, selection, tokens
    fn input(&mut self, event: &UiEvent) -> Vec<Invocation>;         // clicks, keys on its own nodes
}
```

- **Why this boundary:** the guarantee holds (an extension sees only the contract and changes only
  through commands), it works the same on web and desktop, and authors write Rust (owner's language
  ruling).
- **First release:** no third-party extensions. The boundary is proven by building the live tree as
  an extension (the owner called it one, SKIN_DISCUSSION item 1), signed and pre-installed in both
  DNA Calc and the standalone Formula Explorer, where NC04 and E01 use it. Until the extension host
  exists (M7), the live tree is an ordinary `dna-ui` component with the same props, so nothing
  waits for M7.

---

## 12. Agent customisation

- **Same vocabulary.** An agent uses the address grammar, the command registry (with each command's
  JSON schema as its tool description) and the query vocabulary. There is no agent-only door.
- **Permissions per workbook:** Look, Propose (default), Apply then review (Fable §3). Writes in
  Propose are `Proposal`s: a transaction with units, its base revision, its preview and three
  checks (opens in Excel, adds no errors, breaks no run), reviewed in the agent panel. Accepting a
  subset previews that subset (R-44); a stale base is refused at its source (R-47).
- **Looks travel by try-on, the model by proposal** (Fable's example 2). An agent's skin or App page
  is data: it is tried on, kept or discarded; anything it needs in the model (a name, a validation
  rule) arrives as a separate proposal.
- **Attribution.** Every accepted unit is a named undo step with `Origin::Agent`, and appears in the
  trail when the trail is on (ruling 14).
- **The skin harness** is a mode of the existing browser smoke runner (`cargo xtask app smoke`,
  `tools/xtask-harness/src/app.rs:3863`), not a second harness: `cargo xtask app smoke calc
  --skin <manifest> --workbook <file>` validates the manifest, mounts it in headless Chrome and
  Firefox, runs the keyboard scripts through the probe, writes the scene screenshots, and proves
  "presentation only" from the model fingerprint. An agent runs it before it proposes a skin.
- **Where the agent runs:** first an in-app agent panel (the page calls the model service the user
  configures); later, a token-protected local endpoint on the desktop, built like `dnacalc serve`
  (`apps/dnacalc/src/serve.rs`). The AgentPort contract is the same for both.

---

## 13. Web and desktop from one code base

| | Browser | Desktop (WebView2) |
|---|---|---|
| Page | the wasm bundle | the same bundle, served from embedded assets |
| Service | level-1 service in a Web Worker | the same service, native, in the desktop process |
| Add-ins | JS add-ins; native providers refused with a reason | XLL, COM, VBA, .xlam through `calc-addin-host` and the surrogate |
| Files | File System Access or download | native dialogs, recent files, Mark-of-the-Web origin |
| Automation | Office.js projection | COM (`calc-com-host`) on the same `HostSession` |
| Transport | `postMessage` with transferable buffers | `postMessage` for control; tile bytes as WebView2 shared buffers instead of base64 in text (`apps/dna-calc/web/desktop-bridge.js:8,53,57`) |

- **Platform services** are one trait the shell calls through `AppCommand`s; each capability is
  reported in `BookInfo`, so a view shows "not available in the browser: needs the desktop's XLL
  host" instead of failing (R-40, C-34).
- **The desktop service** stays on the STA thread for now, because COM automation shares the
  session (`SessionAccess`, `apps/dna-calc-desktop/src/automation.rs:13`) and calculation already
  runs in 6 ms slices on an 8 ms timer (`TICK_MS`, `app.rs:156`; slices at `:310,762`; the timer
  at `:991`). Section 17.8 says when it must move.
- **Browser-owned keys** (F5, F11, Ctrl+W, Ctrl+L, Ctrl+Tab and the rest of A.5) are published as
  same, explicit alternate, browser-owned or unavailable, per browser.

---

## 14. The website and the app

The aim: the app and the site tell the same stories and link both ways.

- **One set of cast workbooks.** Budget, Sales, Loan, Stock and Inherited become real .xlsx files
  made by a generator in the repository. They are the app's examples and templates, the website's
  stories and pictures, the scene gallery's fixtures, the screenshot review set, and the Explorer's
  examples.
- **App to site.** The handbook already has a page per function, generated from the catalogue
  (`https://handbook.dnacalc.com/functions/ROUND.html`, with `/api/functions/FUNC.ROUND.json`;
  `tools/xtask-harness/src/site/marketing.rs:413-443`). The function browser, signature help and the
  Explorer's help link there, with the same text the app bundles for offline use. New: a page per
  command, keyed by `CommandSpec::help` (`/commands/edit.paste-values.html`), a page per view
  (`/views/notebook.html`), and the cast stories. Help opens the page; offline, the bundled help
  answers.
- **Site to app.** A story's "Open in DNA Calc" link opens the web app at a place:
  `https://app.dnacalc.com/open?example=sales&view=notebook&at=Analysis!A10` (the site's layout
  already names `https://app.dnacalc.com/` as its browser app, `marketing.rs:331`). The desktop
  registers `dnacalc://open?...`, asks before opening a file from a link, and never enables macros
  because of a link. The query grammar is the address grammar plus a skin id, the same one agents
  use. A "Try this formula" link opens the standalone Explorer (`https://onecalc.dnacalc.com/`) with
  the formula and inputs.
- **The teaser.** The site already embeds the Formula Explorer in any page with
  `<div id="onecalc">` (`tools/xtask-harness/src/site/marketing.rs:249,631`); the new skin 0 keeps
  that hook, with a `data-formula` to start from, so every function page can carry a live example.
- **The foregrounded strengths** (decisions, aim of the round) each have an app surface the site
  links to: automation (the views library and COM), engine use (`dnacalc` CLI), cross-platform (one
  bundle, browser and desktop), and agentic tweaking (try-on and the skin harness).
- **Privacy:** the app sends the site nothing but the user's own navigation.
- **A hook for the book** (ruling 8, decided in dc-tojen): "Excel underneath. A better way to see
  it." Offered, not decided here.

---

## 15. Code shape

### 15.1 The new tree

| Crate | Status | Modules |
|---|---|---|
| `crates/calc-skin-api` | new | `target`, `selection`, `command` (Model, View, Calc, Nav), `txn`, `history`, `query` (+ answers), `observed`, `calc`, `event`, `view_part` (re-exports calc-doc-model's stored types), `formula` (level-0 wire), `wire` (envelope; tile codec from `dna-calc-protocol/src/tiles.rs`; Office.js batch messages from `dna-calc-model/src/automation.rs`) |
| `crates/calc-formula-host` | new | `host` (FormulaHost, from dna-onecalc `session.rs`), `trial` (from `fixture.rs`), `providers`, `vba` (a module's UDFs under macro policy), `rtd` (from calc-host `recalc.rs` and dna-onecalc `rtd.rs`), `conditional` (from dna-onecalc `cf.rs`), `trace` |
| `crates/calc-doc-model` | kept, changed | `view_part` (anchors, declarations, prefs, review, trail, session) replaces `layout.rs`'s v1 `SkinLayout`; `WorkbookOp::EditView` replaces `EditSkinLayout` |
| `crates/calc-doc` | kept, changed | the view part's reader and writer over `custom_xml.rs`; the v1 manifest reader and writer deleted |
| `crates/calc-host` | kept, changed | `dispatch(Command)` replaces `SkinIntent`; owns a `FormulaHost`; anchors transformed by structural edits; `preview`; `run_scope` (macro runs); the print plan (pagination moved in) |
| `crates/calc-chart-render` | kept, changed | drawing over `ChartData`, no calc-host dependency |
| `apps/dna-calc-service` | from dna-calc-worker | `server` (one dispatcher), `queries`, `publication`, `journal`, `book` (from workbook.rs), `names`, `search`, `autofit`, `drawn`, `picture`, `omml`, `clock`; `bin/worker` |
| `apps/dna-formula-service` | new | level-0 server; `bin/worker` |
| `apps/dna-grid-core` | from dna-calc-model | `selection`, `editor`, `geometry`, `viewport`, `render_plan`, `tile_mirror`, `drawn_shapes`, `sparklines`, `controls`, `address`, `shape_colour` |
| `apps/dna-skin-kit` | new | `book`, `live`, `commands`, `keymap`, `focus`, `draft`, `frame`, `declaration`, `probe`, `transport` (from page port.rs) |
| `apps/dna-ui` | new | components (7.1); the grid painter, panes, decoration, alignment and autofit from `apps/dna-calc/src/grid/`; the formula field's tokenizer, highlight, completion and signature help from `dna-formula-ui` (page-side, over the catalogue, 5.1) |
| `apps/dna-shell` | new | frame, Mast, Strip, Deck, command search, file, print, history, macro bar, settings, studio, agent panel |
| `apps/dna-skin-{classic,inspector,notebook,map,app,forms,explorer}` | new | one crate per skin: its `install`, root and panel components, commands, keymap layer and manifest JSON |
| `apps/dna-strand` | kept, extended | `resolve`, runtime CSS properties |
| `apps/dna-calc`, `apps/dna-onecalc` | kept names, new contents | thin web entries: registry of skins, shell, platform |
| `apps/dna-calc-desktop`, `apps/dna-onecalc-desktop` | kept, trimmed | one shared desktop host library over `calc-platform-desktop`, each exe with its own service level |
| `apps/dna-ext-sdk` | new, later | extension guest SDK |

The package names `dna-onecalc` and `dna-onecalc-desktop` stay, because the site packer, the sweep's
ceiling step and the bundles name them (dc-sdzi1 keeps internal OneCalc names).

### 15.2 Kept, rewritten, thrown away

Line counts from `wc -l` at `4120ba262` (production and in-file tests together).

| Code | Lines | Fate | Why |
|---|---:|---|---|
| `apps/dna-calc/src/controller.rs` and `controller/*` | 3,201 + 2,622 | thrown away | the god object the kit replaces; its rules (type-ahead `input_pending`/`after_input` at `controller.rs:512,525`, focus tickets, surface transfer) are ported as kit code |
| `apps/dna-calc/src` skins: notebook, tree, atlas, canvas, ledger | 3,729 | thrown away | replaced by the views; ledger folds into data frames, atlas into Inspector inventory and the map overview, tree into the outline panel and live tree, canvas into the map |
| `apps/dna-calc/src/grid/mod.rs` (UI) | 4,296 | rewritten | the GridSurface component; its behaviour comes back through the ported smoke cases |
| `apps/dna-calc/src/grid/` painter, panes, decoration, alignment, autofit, axis, icons, image, gdi_advances | 4,150 | kept, moved to dna-ui | Excel paint fidelity work |
| `apps/dna-calc/src` shell and feature UI: app, commands, review, charts, printing, format, pivots, names, tables, sheets, page_setup, files, clipboard, addins | 12,657 | thrown away | written fresh as shell and Classic dialogs on components |
| `apps/dna-calc/src/workbook_command.rs`, `wire.rs`, `model_adapter.rs`, `presentation.rs` | 914 | thrown away | the translation layers one command enum removes |
| `apps/dna-calc/src/input_adapter.rs`, `port.rs`, `style.rs` | 1,195 + 447 + 78 | kept as logic in the kit and dna-ui | browser input, transport and the CSSOM rule (`style.rs:3-9`: the CSP allows no inline style) |
| `apps/dna-calc/tests/smoke/cases.rs` | 18,132 (1,053 cases) | ported once to the probe | the behaviour record of Classic |
| `apps/dna-calc-model` projection, runtime, command, view, tree, state, outbox, canvas, ledger | 4,783 | thrown away | replaced by calc-skin-api and the kit; `EditSession` (`state.rs:54`) moves to the kit first |
| `apps/dna-calc-model` selection, geometry, render_plan, tile_mirror, editor, viewport, drawn_shapes, sparklines, controls, address, shape_colour | 9,258 | kept as dna-grid-core | DOM-free, tested, Excel-faithful |
| `apps/dna-calc-model/src/printing.rs` | 1,285 | kept, moved into calc-host | `calc-host/src/print_plan.rs` already calls its `paginate`; the page reads the `PrintPlan` answer |
| `apps/dna-calc-model` tiles, automation, names, clipboard, presentation, format, parts, transport, lib | 1,187 | moved with their users | tile and Office.js messages to calc-skin-api::wire, the rest to the kit or deleted with the projection |
| `apps/dna-calc-protocol` | 524 | folded into calc-skin-api::wire | one contract crate |
| `apps/dna-calc-worker/src/server.rs` | 10,027 | rewritten as the dispatcher; its tests kept where they test service behaviour | the 1,200-line `intent()` (`server.rs:2323-3521`) and the per-skin request arms go |
| `apps/dna-calc-worker` canvas, ledger, tree | 291 | thrown away | per-skin projections |
| `apps/dna-calc-worker` workbook, publication, names, autofit, search, cells, parts, format, message, worker, model_clock, transport, drawn, picture, omml, custom_geometry, printing, services | 7,510 | kept in dna-calc-service | view-neutral services; `printing.rs` becomes the `PrintPlan` handler |
| `apps/dna-onecalc` surface, port, bridge, panels, shown, grid | 4,852 | thrown away | the Explorer's own port, link and surface (brief item 9) |
| `apps/dna-onecalc` session, cf, rtd, trace, fixture, file | 4,523 | kept; all but `file.rs` move into calc-formula-host | the level-0 evaluator route; the session file stays with the Explorer app |
| `apps/dna-onecalc-desktop` host, app | 3,286 | rewritten onto the shared desktop host and the level-0 service | one desktop shell |
| `apps/dna-onecalc-desktop` surrogate, rtd, settings | 1,573 | kept | add-in process isolation |
| `apps/dna-formula-ui` | 4,001 | logic kept (formula_field, editor, result, display, parts: 3,505), Leptos views rewritten in dna-ui; crate deleted | removes its dependency on all of dna-calc-model |
| `crates/calc-doc` v1 manifest reader and writer, `crates/calc-doc-model/src/layout.rs` (812), hidden locator names | — | replaced by the view part | ruling 13 |
| `crates/calc-host` `SkinIntent` (`session.rs:145-242`), `presentation_commands.rs` notebook placement | — | replaced by `dispatch(Command)` and view-part edits | one command path |

Totals, from the rows above: 33,049 lines are thrown away (controller 5,823, old skins 3,729, shell
and feature UI 12,657, translation layers 914, model projections 4,783, worker projections 291,
Explorer surface 4,852); 22,203 are kept and moved (grid core 9,258, pagination 1,285, painter
4,150, worker services 7,510); 14,323 are rewritten (grid UI 4,296, worker server 10,027); and the
Explorer's 4,523 lines of evaluator route move below the page.

No step keeps the old and the new UI side by side in one crate. The old page stays as it is until
the new Classic replaces it in one switch (section 16, M2); the old crates are deleted in that
change.

---

## 16. Build order

Each milestone ends with something published for feedback (ruling 10). Sizes are relative; there
are no dates.

| Milestone | What gets built | What shows it works |
|---|---|---|
| **M0 Contract** | calc-skin-api; calc-formula-host split; calc-host `dispatch(Command)`, anchors, view part, history with presentation; dna-calc-service and dna-formula-service; dna-skin-kit with `Book`, `Live`, registry, keymap, draft, frame, probe | a contract suite run natively and in the browsers against the in-process service: every command kind, undo across kinds, stale refusals, anchors through structural edits, invalidation, budgets |
| **M1 Components, shell, skin 0** | dna-ui foundations, dna-shell frame, dna-strand resolve; the Formula Explorer skin; standalone builds web and desktop | the teaser on the site; the existing Explorer tests ported; the sweep's ceiling step shows no calc-engine in the Explorer closure |
| **M2 Classic** | GridSurface from the kept logic, Deck with Excel's key tips, dialogs, Classic keymap; the Explorer inside DNA Calc; the old page deleted | the ported smoke cases that passed before pass again; the UI_REVIEW parity table re-run; corpus open in Classic with no value changed |
| **M3 Inspector** | the tools as components and panels | Sam's Sales trace (S06/S07) and Inherited's inventory over the cast workbooks |
| **M4 Notebook** | the notebook skin and its Jupyter keymap, the backing-layout allocator (scalar zone, array lanes, relocation), held drafts, prose with bound values | NC01-NC07 over the cast Sales workbook; the Jupyter key list run through the probe; the saved file calculates the same in Excel (a Windows step) |
| **M5 App and forms maker** | the page renderer, the App view, the forms maker view, notebook excerpts | Loan (F05, A01-A03, S10, S11); the U05 three-line App built by an agent through try-on; NC07's write-up as an App page |
| **M6 Model map** | the Graph component with level of detail, then the map skin | Stock (F04, M01, M02) and Inherited at scale (M03-M05) |
| **M7 Customisation and extensions** | the studio (manifest editor, token and contrast view, keymap editor), the agent port, the skin harness, the extension host; the live tree as the first extension | U01-U04; an agent restyles Classic and the harness proves presentation only |

**Hours per view.** Once M0 and M1 exist, the first usable Explorer, Inspector, Notebook, App and
forms maker views are each hours of work: a manifest, a root component composed from the library,
and their commands. Three pieces are not hours, and are said plainly so nobody plans otherwise:
GridSurface (days, because it carries the Excel behaviour, and the conformance list is long-running
work after that), Graph (days, because of layout and level of detail at scale), and the notebook's
backing-layout allocator with its relocation fixes (a day or two, because each fix is a previewed
model transaction with its own tests).

**What runs between M0 and M2.** Master must build and the published app must keep working, so
for two milestones the old page and the new contract share calc-host. That is the only overlap,
and it is bounded:

- M0 adds `dispatch(Command)` beside `SkinIntent`; the old worker keeps calling `SkinIntent` until
  M2 deletes both.
- M0 replaces `SkinLayout` with the view part. Its only users are the old notebook, tree and canvas
  skins, which are thrown away anyway, so M0 deletes them (and their smoke cases) at once. The old
  page runs Classic, the ledger and the atlas until M2.
- M1's Explorer replaces the old one outright (both builds), because nothing else uses
  `dna-onecalc`'s port.
- M2 switches DNA Calc to the new page in one change and deletes `dna-calc-model`'s projection,
  `dna-calc-protocol`, `dna-calc-worker`, the old page and `SkinIntent`.

---

## 17. The hard parts

### 17.1 Undo across a macro run (ruling 4: design for it, do not promise it in the first release)

Excel clears its undo list when a macro changes the workbook, and offers `Application.OnUndo`
for a macro to register its own undo. Today calc-host records each automation write as its own
undo step (`session.rs` `edit_coordinates`, `:1544`, records every commit, whatever its ingress),
which is not Excel's behaviour. The history commits forward: an undo is a new transaction of
inverse edits, and a structural step brings its earlier revision back whole
(`crates/calc-host/src/history.rs:1-12,59`; `command::restore`, `command.rs:1240`).

- **First release, Excel's behaviour:** a macro run that changes the workbook clears the history,
  and the history list says why it was cleared. `Application.OnUndo` is implemented as Excel's (the
  descriptor lists it; today it has no handler, so it answers `NotModelled` like every member
  without one).
- **The design for more:** the host opens a run scope at `MacroRun::Started`. Every commit inside
  the run is recorded into the scope; at `Ended` the scope becomes one step, "Macro: StressRate",
  whose inverse replays the inner inverses in reverse as one transaction, or restores the
  checkpoint when an inner step is structural. The checkpoint is cheap: `ModelRoot` shares its
  pages, definitions and layout through `Arc` (`crates/calc-doc-model/src/root.rs:264-272`, and
  `ModelRoot::pin`, `:289`), so a checkpoint is a pinned root, not a copy. The step lists what it cannot reverse (`ExternalEffect`):
  files written, network calls, add-in calls with effects, other workbooks changed, application
  settings.
- **What makes it hard:** restoring a structural checkpoint replans the engine, a cost that grows
  with the workbook (`command::restore`); the VBA project's module-level variables are not
  restored, so the macro's own state can disagree with the workbook after undo, and the step must
  say so; a run that touches several open workbooks needs one step per workbook or refuses undo;
  undo raises workbook events, and a Worksheet_Change handler can start another run (the closest
  Excel analogue, the events an ordinary Undo raises, has to be observed in Excel first); and
  Guarded mode's captured action plans (G01-G03) are the easier case, because the plan is known
  before the run.

### 17.2 Big workbooks

The Enron population holds 20.7 million formula cells; Inherited stands for a 40-sheet,
300,000-formula file.

- **Known costs in today's route** (harvest 17, findings 2-3, and the worker survey): any
  conditional-format rule makes every commit reset every subscription; each tile root exists four
  times (host, worker, page committed, page staging); geometry for all sheets is rebuilt and cloned
  per subscription on every collect (`server.rs:4214-4245`); notebook paging starts from every
  input cell of the sheet; tile atoms are JSON inside the binary frame and base64 again on the
  desktop. Each is a defect to fix in M0 or M2, not a reason to change the contract.
- **The contract's answer:** every read is budgeted and returns coverage or a declined answer with
  smaller scopes; the map and the Inspector start from one frontier or from sheet groups; open
  shows saved values first and labels them Saved.
- **Still open:** dependency summaries by block at Enron scale need an index the engine does not
  yet keep; the budgets of Pro 19 §6 are targets, measured on the reference machine, never
  per-commit checks.

### 17.3 Conformance with Excel's keyboard and mouse

- Appendix A of Pro 19 has 121 cases; each needs Excel's observed result recorded on Windows, in
  its context (Ready, Enter, Edit, Point, End, Extend, Add), and then the same keys through the
  probe in Chrome, Firefox and WebView2.
- Key tips exactly as Excel's (ruling 11) need Excel's key-tip tree recorded per build, including
  the paths of commands DNA Calc does not have, which then show a refusal instead of a different
  command.
- The mouse is harder than the keys: the fill handle's double-click extent, drag with modifier
  changes mid-drag, the overwrite warning, range picking across sheets, dragging a reference's
  corner while editing. These are gesture recognisers in GridSurface with recorded fixtures.
- The browser keeps some keys; AltGr layouts turn Ctrl+Alt chords into characters; IME
  composition must not commit an entry. Each has a published per-browser result.
- There is no recording route for Excel's UI behaviour yet (the capture route records
  calculations). Building one is part of M2.

### 17.4 Preview with recalculation

Structural preview (which cells, references, spills and names change) is cheap and comes first.
Value preview needs the transaction calculated without publishing: either a forked engine (cost
unknown; the engine has no fork today) or apply, calculate and roll back inside the host without
delivering deltas, which is unsafe with volatile functions, RTD and macros. The design: value
preview only within a budget and only for workbooks with no volatile, RTD or macro dependency in the
affected closure; otherwise declined with "commit and undo" offered. Agent subset acceptance (R-44)
depends on it.

### 17.5 Anchors and the Excel round trip

Unnamed anchors break when an Excel user moves cells; reconciliation recovers what it can prove
and leaves the rest visibly broken. Retention of the part across Excel's save routes is tested in
dc-1nbp.22 and has not been shown yet.

### 17.6 The notebook's backing layout

Scalar rows and array lanes avoid most spill collisions, but not all. A scalar that starts to
spill runs into the next scalar's row, and a lane that grows wider needs a column insert, which
moves everything to its right on that sheet. On a notebook's own sheet that stays local. On an
existing sheet (W04's Summary) a column insert moves the user's own content, so the preview lists
what moves and the person chooses between the fixes (9.3); nothing is ever moved silently, and no
calculation ever triggers an edit. The layout is decided at commit from the preview's shape, so a
formula whose shape changes later (a FILTER that returns one row today and 4,000 tomorrow, NC06)
will meet `#SPILL!`, exactly as it would in Excel. Reordering cells changes reading order only; a
notebook whose reading order differs greatly from its backing layout can confuse an Excel reader,
and "Tidy layout" (one model transaction of cut-and-paste moves) is the answer.

### 17.7 Selection authority

The page moves the cursor before the host knows. The sequence numbers keep commands ordered, but a
macro's `Range.Select` arriving between two keys must win without losing the user's next keystroke.
The type-ahead queue and the host's selection order need one shared test suite.

### 17.8 Desktop threading and transport

The desktop service runs on the STA thread with COM. A long, uninterruptible add-in call blocks COM
and the message pump. Moving the service to its own thread needs COM calls marshalled to it; that is
needed when a measured stall exceeds the input budget, and not before. Tiles move to WebView2
shared buffers in M0.

### 17.9 The extension sandbox

A wasm guest that returns a render tree adds a hop per update. For the live tree that is small; for
anything grid-sized it would be too slow, so the vocabulary has no per-cell element.

### 17.10 Accessibility of painted surfaces

The grid and the map paint on a canvas. Both need a parallel semantic layer with real coordinates
(R-55) and coalesced announcements (R-59). The grid's exists (4,096 cells); the map's does not.

### 17.11 Mixed revisions across many live reads

`consistent()` waits for one calculated revision. Under continuous RTD updates that revision may
never settle; consistent reads then pin the last complete revision and say so.

### 17.12 Several workbooks

A macro or a paste can touch more than one open workbook; history, selection and anchors are per
workbook; cross-workbook references show external-link state. The contract carries `BookId`; the
first release opens one workbook per window.

---

## 18. Not taken, and why

| Idea (source) | Why not |
|---|---|
| FerroUI as the native UI (fu1 evaluation) | Ruling 7: not at this stage; Leptos and WebView2 stay |
| Per-view undo stacks, or one model history plus a presentation history per view (Fable §3) | The agreed reading (README: one workbook, one selection, one undo history) and R-21. One ordered history is predictable; the undo notice names a step made in another view, so a box drag undone from the grid is never silent |
| Hidden defined names as anchors (current v1 manifest) | Ruling 13 |
| A separate notebook reading mode (Pro 21 Author/Read) | Ruling 6; reader-style output is an App page |
| The six current views as top-level modes (ledger, atlas, tree, canvas) | Folded into the five views, as Pro 19 §10 and the code map show |
| Code in files: scripts, JS components, web components | No code in files; product code is Rust |
| Per-skin projections built in the service | A visual change must not touch service code (brief item 3); neutral queries instead |
| Level 0 as a cargo feature of calc-host (brief option a) | Features unify across the workspace; a crate boundary is checked |
| A raw event bus as the skin's main input | Live values carry revision, provenance and coverage; raw events lose them |
| Free tiling of views, or a tab per view (Fable §3) | At most a view and a companion, plus panels |
| A second declarative UI language (XAML-like templates) | Manifests compose components; templates would be a second component model |
| Notebook cells as run-once scripts with execution counts | The workbook's calculation decides; Jupyter's interaction, Excel's calculation |
| Confidence percentages on recognised patterns (Pro 19 §5) | Four states: From workbook, Suggested, Confirmed, Needs review |
| Per-group "directors" on the map (linked-boxes previews) | Not an Excel mechanism (Pro 19 push-back 1); the RTD clock and Excel's iteration only |
| Animated firing order without host evidence (Carbon M-05 pulse) | A dependency picture is not an execution recording (push-back 2) |
| Automatic rewriting in the formula lens | The lens is read-only; "Rewrite as LET" is an explicit previewed model change (push-back 3) |
| Automatic App publication from inference | Author-confirmed bindings only (R-68) |
| Ctrl+1 to Ctrl+5 for views (Carbon) | Excel's keys (Ctrl+1 is Format Cells); ruling 12 uses Neon's Ctrl+Alt chords |
| Ctrl+Alt+5 for App | Excel binds it to floating-shape navigation |
| Keeping `SkinIntent`, the wire `Intent` and the page `WorkbookCommand` | Four names for one operation; one `ModelCommand` |
| Keeping the Explorer's own port, link and shell | Brief item 9; it becomes skin 0 on level 0 |
| A step-by-step migration of the current skins | Ruling 10 |
| Third-party extensions in the first release | The boundary is proven with one built-in extension first |
| WebGPU grid painter now | The render plan stays a display list ready for it; Canvas2D first, as today |
| Switching to a sheet's preferred or last-used view on a tab click (GPT R-14, GPT P19-0034) | The badge rule (Fable R-19 and push-back 4, Neon push-back): a click on a tab never replaces the surface; the file's preference applies once, at open |
| Coded widgets as signed Web Components (Carbon) | Extensions are Rust compiled to wasm in a worker sandbox with no DOM (section 11): the language ruling, and a component that holds the DOM can read and change any part of the page |
| Dual anchors with opt-in hidden `_` names (Carbon open question 2) | Ruling 13: anchors live in our part, keyed to cells and the user's own names |
| Copy-on-write forks of the workbook for staged change sets (Carbon C-16) | The engine has no fork; previews are structural first and recalculated only within a budget (17.4), with "commit and undo" offered otherwise |
| A macro run as one undo step in the first release (Carbon R-35, Fable R-98) | Ruling 4: designed (17.1), not promised; the first release does what Excel does |
| The agent trail off by default (Fable open question) | Ruling 14: kept in the file, can be turned off |
| Esc reverting an edit in the notebook (Strand's fixed rule as Fable states it) | Ruling 6: the notebook is Jupyter's, where Esc leaves edit mode and keeps the text. The safety the rule protects holds in every view: Esc never commits anything |
| Completion and signature help as service calls (level 0) | A keystroke would wait for a round trip; the page completes over a catalogue the service sends (5.1) |
| The forms maker as a mode of the App view | Owner on Pro 21's notebook: author and read modes blur. The forms maker is its own view over the same live page |
| Switching views automatically on a tab click in a published App or guided mode (Neon open question 2) | The badge rule; an App opens by the person's choice or by the file's preference at open |
| A COM library for the views verbs (FRONT_END_SCOPE §3) | `Application.Run` names need no reference and no registered server, and fail with a trappable error in Excel (Fable R-99); an early-bound reference would not even compile there |

---

## 19. Questions for the owner

1. **Notebook prose for Excel readers.** This proposal keeps prose in the view part only. Should a
   notebook also write its prose into the backing sheet as text cells, so an Excel reader sees the
   write-up (as an optional mirror, like notes under ruling 5)?
2. **The agent's endpoint.** In-app agent panel first, and the local desktop endpoint later, as
   proposed? The local endpoint lets outside agents drive DNA Calc, which is powerful and needs its
   own security review.
3. **The extension trust root.** Only DNA Kode's signing key in the first release that has
   extensions, with user-added keys later?
4. **Esc in the notebook.** This proposal takes Jupyter's Esc (leave edit mode, keep the unrun text,
   marked "edited, not run"), because ruling 6 makes the notebook Jupyter's. The alternative is
   Excel's Esc (revert the edit), which keeps one rule across all views but is not how Jupyter
   behaves.

---

## 20. The C-needs on the contract

Pro 19's Appendix C lists 34 needs the design has of the system underneath (the brief asks the plan
to map each onto the layers). "Today" says what already exists in the code; "new" is work in the
milestone named.

| C | Need (short) | Where in this architecture | Today |
|---|---|---|---|
| C-1 | Identify sources; know whether one still exists | `SourceRef`, `AnchorId` (4.1); anchors and reconciliation (10.2, 10.3) | `Target` has five kinds; anchors new (M0) |
| C-2 | Exact formulas, values, shown text, formats, notes, settings | `Cell`, `Book` queries; tiles (4.7) | tiles and `Summary` exist |
| C-3 | Saved, current, dirty, calculating, cancelled, unavailable, by revision | `Provenance`, `CalcStatus`, `Observed` (4.8) | the `Calculation` event; provenance new (M0) |
| C-4 | Be told what changed, formats and spill shape included | `Committed { touched }`, `Calculated { changed }`, tile deltas, `Invalidated` (4.7, 4.9) | tile deltas exist |
| C-5 | Incremental reads with progress; empty, not loaded, failed | tile admission and completion; `Observed::Loading`/`Failed`; saved values first (17.2) | admission and completion exist |
| C-6 | Inventories with hidden and protection state, extents, counts | `Book`, `Inventory`, `Names`, `Tables`, `Objects` with `Coverage` (4.7) | partial (atlas); new as neutral queries (M0) |
| C-7 | Precedents and dependents by block, dynamic references marked | `Dependencies` (4.7) | outline and dependents per skin; new (M3) |
| C-8 | Exact membership and exceptions behind a grouped edge | `Dependencies { group }`, `Regions` (4.7) | new (M3, M6) |
| C-9 | Formula structure, reference identity, bindings, exact spelling | calc-syntax spans; `Trace`; page tokenizer (5.1) | `TraceFormula` and the formula field exist |
| C-10 | Editability: spills, legacy arrays, tables, pivots, protection, objects | `Cell` facts, `CanEdit`, `Refusal::Protected`/`Invalid` (4.5) | `CanEdit` exists (`server.rs:865`) |
| C-11 | Preview and apply Excel edits with exact consequences | `preview(&Transaction)` and `Preview` (4.5, 17.4); `ModelCommand` | apply exists; preview new (M0 structural) |
| C-12 | Atomic sets, stale refusal, conflicts at their source | `Transaction { expect, units }`, `Refusal::Stale` (4.5) | stale-selection refusal exists (`server.rs:690`) |
| C-13 | Undo units across model and presentation; irreversible effects named | one `History` with `ViewEdit` (4.4, 4.6); `StepInfo::external`; `Refusal::Irreversible` | `CellEdit::Layout` inverses and `Irreversible` exist |
| C-14 | Recover committed work and drafts apart, never twice | Recovery (4.6) | in-session deduplication exists; recovery new (M2) |
| C-15 | Trials in a caller context; trial vs temporary workbook state | `FormulaHost::evaluate(Trial { caller })` (5.1); value preview (17.4) | the Explorer's `Session` exists |
| C-16 | Whether a provider is safe to preview, needs context, has effects | provider facts in `FunctionCatalogue` (5.1) | new (M1) |
| C-17 | Add-in functions, metadata, provider, version, permission, host | `FormulaHost::functions`, `ProviderRegistry` (5.1) | `UserFunctions`, `Addins` exist |
| C-18 | Results apart from number-format and conditional-format evaluation | `ResultView`, `cell_text_localized`, `FormulaHost::conditional` (5.1, 9.6) | exists in the Explorer |
| C-19 | Mode, scope, iteration, cycle members, observed stop reason | `CalcStatus`, `CalcCommand::SetIteration` (4.8) | mode and iteration exist; cycles new |
| C-20 | Observed order only when observed | `CalcChain` answers `NotObserved` unless recorded (4.8) | new (an honest refusal first) |
| C-21 | Link and provider identity, availability, last value, refresh state | `Provenance::External`, `Inventory`, `Live` (4.7, 4.8) | partial |
| C-22 | Live updates with times, coverage, coalescing | `BookEvent::Live`, `Live { topic }` (4.7, 4.9) | RTD admission exists; observation new (M3) |
| C-23 | Exact filtered and contribution queries; bounded row windows | `Rows { source, filter, window }`; `Trace { budget }` (4.7) | new (M3, M4) |
| C-24 | Pivots, charts, controls, scenarios, validation, print settings | `ModelCommand` groups; `ChartData`; `PrintPlan` (4.3, 4.7) | mostly exist as today's intents |
| C-25 | Macro presence, trust, run boundaries, effects; presentation requests | `Book`, `MacroRun`, `Origin::Macro`, the views library (4.9), run scope (17.1) | presence and trust exist; run scope new |
| C-26 | Versioned view declarations in the file, unrelated parts preserved | the view part over calc-doc's custom XML route (10.1) | the route and v1 exist |
| C-27 | Unsupported presentation versions kept for recovery | kept byte for byte, Classic opens (10.1) | new (M0) |
| C-28 | Clipboard with origin and kinds; loss declared | the clipboard (4.3) | system clipboard and Paste Special exist |
| C-29 | One completed-result snapshot for print and export | `book.consistent()`, `PrintPlan` (4.8) | new (M2) |
| C-30 | Commands, states, coordinates, selections reachable accessibly | the registry (6.3), the probe (6.7), the semantic layer (17.10) | the grid's 4,096-cell layer (`render_plan.rs:1239`) |
| C-31 | Cancel or bound long work; navigation stays responsive | `CalcCommand::Cancel`, sliced calculation, `Budget` (4.7, 5.3) | sliced calculation exists |
| C-32 | Estimates and exact totals before expanding | `Budget`, `Observed::Declined { alternatives }`, `Coverage` (4.7) | new (M0) |
| C-33 | Attribution, revision comparison, reversal against now | `Origin`, `StepInfo`, `Revert`, proposals (4.5, 4.6, 12) | new (M0, M7) |
| C-34 | Unsupported capability apart from invalid content; originals kept | `Refusal::NotModelled` vs `Invalid`; capabilities in `BookInfo` (13) | typed refusals exist |

Every need lands on a named part of the contract. None needs the engine to do more than Excel: C-20
is answered by a refusal until an order is actually recorded, and C-7 and C-8 report what the
dependency graph already knows.
