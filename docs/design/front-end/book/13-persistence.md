# Part XIII. Persistence

Clusters: `CL-13-01`–`CL-13-09` · Governing decisions: `D-05`, `D-13`, `D-14`, `D-83`, `D-86`, `D-87` · Architectural contract: [SKIN_ARCHITECTURE.md §12](../SKIN_ARCHITECTURE.md) · Mockups: [Pro 21 W04](http://127.0.0.1:8720/p21/workbook.html#W04), [Pro 21 W05](http://127.0.0.1:8720/p21/workbook.html#W05), [Pro 21 O02](http://127.0.0.1:8720/p21/workbook.html#O02)

---

## 13.1 The CustomXML View Part (`urn:dnacalc:views:v1`) and Excel Round-Trip (`CL-13-01`, `D-83`)

DNA Calc writes ordinary `.xlsx` and `.xlsm` packages grounded in the carried-forward OOXML standards index and CalcDoc fixture disposition (`docs/FORMAT_REFERENCES.md`) `[FRAMING-0077]`. Every presentation artefact that is not part of Excel's own worksheet, workbook, chart, table, or calculation XML—preferred and supported views, per-sheet layout bindings, Notebook cells and `RichProse`, Model map box positions and hierarchy groupings, App/Forms `dna.page/1` declarations, embedded `dna.skin/1` manifests, confirmed pattern readings, anchored review notes (`D-05`), and the opt-out agent change trail (`D-14`)—lives strictly inside **one versioned OOXML CustomXML item** so Microsoft Excel opens, calculates, edits, and re-saves the workbook with zero repair prompts and identical calculated values (`R-9`, `R-105`, `C-26`) `[FRAMING-0105, P19-FABLE-0522, P19-GPT-0325, P19-NEON-0199]`.

### 13.1.1 Package Allocation, Schema Versioning, and Forward Compatibility

See [Pro 21 W05 — File: preferred and supported views](http://127.0.0.1:8720/p21/workbook.html#W05) and [Pro 21 O02 — File: save with the views](http://127.0.0.1:8720/p21/workbook.html#O02).

1. **Dynamic standard OOXML allocation (`D-83`):** Rather than hardcoding `/customXml/item1.xml` (which would collide with workbooks already carrying enterprise or add-in CustomXML parts) or inventing non-standard package paths, `calc-doc/src/custom_xml.rs` (`CustomXmlItem`) scans existing `/customXml/item*.xml` parts, locates any existing `urn:dnacalc:views:v1` item by namespace, or allocates the next free `/customXml/itemN.xml` + `/customXml/itemPropsN.xml` pair with proper workbook `.rels` relationships `[P21-0093, D-83]`. All unrelated workbook parts, macros (`vbaProject.bin`), connections, and third-party CustomXML parts are preserved untouched (`C-26`) `[P19-GPT-0325]`.
2. **Canonical XML envelope mapping 1:1 with JSON (`SKIN_ARCHITECTURE.md §12.1`):**
   ```xml
   <dnaViews xmlns="urn:dnacalc:views:v1" version="1" writer="DNA Calc 0.2" model="sha256:8f4e...">
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
     <readings><!-- confirmed / corrected pattern readings + evidence fingerprints --></readings>
     <notes><!-- anchored review notes and findings (D-05) --></notes>
     <trail enabled="true"><!-- accepted/rejected/undone agent proposal unit records (D-14) --></trail>
     <requires><extension id="dnakode.live-tree" version="^1.0"/></requires>
   </dnaViews>
   ```
   Inside `<decls>` and `<skins>`, view documents (`dna.page/1`, Notebook chains, Map layouts) and skin manifests (`dna.skin/1`) are stored in the exact canonical JSON edited in Customisation Studio and `.dnaskin.json` files (`D-83`). Unknown XML elements and attributes within `version="1"` are retained byte-for-byte on round-trip.
3. **Opaque retention of newer versions (`W05` exception, `C-27`):** If a workbook carries a future major version (for example, `urn:dnacalc:views:v3` or `version="3"`), DNA Calc **retains the CustomXML part opaquely without overwriting or discarding it**, opens the workbook safely in Classic, displays a non-modal banner (`View declarations require newer DNA Calc (v3) · Preserved untouched on save · [Export raw view XML]`), and refuses destructive view-part overwrites unless the user explicitly exports a copy (`C-27`) `[P19-GPT-0326, P21-0094]`.
4. **Retiring prototype v1 (`urn:dnacalc:view-manifest`):** Because zero user or corpus workbooks use the prototype `manifest.rs` / `layout.rs` format, both files (`1,989` lines) are deleted outright at Milestone 3 rather than carrying legacy migration shims (`D-10`, `D-13`).

---

## 13.2 Zero Hidden Names and Deterministic Four-Tier Anchor Reconciliation (`CL-13-02`, `D-13`)

Decider Ruling #13 (`D-13`, overriding earlier drafts in `FRONT_END_SCOPE.md` and `FRONT_END_IMPLEMENTATION_BRIEF.md`) establishes an unconditional invariant: **DNA Calc never writes hidden `_dna.*` defined names into user workbooks** `[FRAMING-0178, P19-FABLE-0232]`. Hidden names pollute Excel's Name Manager via automation/XML inspection and violate the rule that every model mutation is visible and user-reproducible.

Instead, every non-grid reference stores a `<anchor>` in `urn:dnacalc:views:v1` combining stable sheet identity (`sheet_key`), user-authored `Name` / `Table` / `Object` identity where present, last-known A1 address (`ref`), and a lightweight structural `<fingerprint>` (R1C1 formula hash, nearby row/column header label, and dimensions) (`C-15`) `[P19-CARBON-0117, P19-CARBON-0221, P19-FABLE-0226, P19-GPT-0075]`.

### 13.2.1 Reconciling Anchors After External Edits in Excel

While editing inside DNA Calc, `calc-doc-model` updates `<anchors>` synchronously alongside row/column insertions, sheet renames, and block moves, updating `model="sha256:..."` on save. When a workbook is opened:

- **Fast path (`model` digest matches):** Every anchor stands `Bound` as saved.
- **Reconciliation path (`model` digest differs because the file was edited and saved in Excel or another tool):** An A1 address surviving at the same coordinate does **not** prove the target survived (`P19-GPT-0075`). Each anchor resolves through four deterministic tiers and **never guesses**:
  1. **Tier 1 — User `Name`, `Table`, or `Object` still exists:** Resolves directly to Excel's updated range target (`Bound`). This is why Notebook and Model map make naming outputs effortless (`Pin with a name` writes a normal, visible workbook name through the Preview Seam) `[P19-FABLE-0232]`.
  2. **Tier 2 — Stored `ref` address still matches `<fingerprint>`:** `Bound`.
  3. **Tier 3 — Unique `<fingerprint>` match elsewhere in the sheet's used range** (for example, 3 rows were inserted above an unnamed block in desktop Excel): Marked **`Relocated { from, to }`** with a non-modal Strip/card notice (`Moved from B12 -> B15 in external edit · [Confirm] [Rebind]`) `[P19-CARBON-0117, P19-CARBON-0221]`.
  4. **Tier 4 — Zero matches or multiple ambiguous matches:** Marked **`Orphan`** (`Source unresolved · [Rebind…] [Remove card]`). Orphaned cards, ports, and prose chips remain visible in place so authored context is never silently deleted or snapped to a wrong neighbouring cell (`R-62`, `R-77`) `[P19-FABLE-0226, P19-GPT-0075, P19-GPT-0292]`.

---

## 13.3 Notes, Findings, Agent Change Trail, and Local-Only Session State (`CL-13-03`, `CL-13-07`, `D-05`, `D-14`)

1. **Anchored Notes and Review Findings (`<notes>`, `D-05`):** Auditor notes, confirmed override reasons (`Noted override` on structural exceptions), and review findings persist inside `<notes>` in `urn:dnacalc:views:v1`, anchored via `<anchors>` so they travel with the `.xlsx` without altering worksheet cells `[FRAMING-0170]`. An explicit opt-in **`Mirror findings to Review sheet`** action writes a standard visible Excel worksheet through the Preview Seam when colleagues without DNA Calc need to read the log in Excel (`D-05`).
2. **File-Persisted Agent Change Trail (`<trail>`, `D-14`):** Accepted, rejected, and subsequently undone agent `ProposalUnit` summaries (timestamp, agent identifier, user prompt summary, target addresses, before/after formula summary, and disposition) persist in `<trail enabled="true">` inside `urn:dnacalc:views:v1` `[FRAMING-0179, P19-GPT-0098]`. Users and organisations with privacy constraints can toggle **`Save agent trail in workbook: Off`** (`PresentationChange::Trail`), stripping `<trail>` cleanly without touching `<decls>` or `<notes>` `[FRAMING-0179, P19-GPT-0098]`.
3. **Local-Only Transient State and Crash-Recovery Journal (`CL-13-07`, `R-47`, `C-4`):** Viewport scroll positions, Model map pan/zoom camera, splitter widths, Inspector live recordings, and the **incremental crash-recovery delta journal** live strictly in local storage (`OPFS` in browsers; profile directory on desktop)—never inside `.xlsx` (avoiding noisy binary file diffs when merely reading a file; Classic's frozen panes, sheet zoom, and active cell stay in Excel's standard `<sheetViews>`) `[P19-GPT-0098, P19-NEON-0237, P19-NEON-0242]`. After an abnormal exit, reopening the file surfaces a non-modal **`Unsaved edits recovered (2 committed changes, 1 held draft) · [Restore] [Inspect diff] [Discard]`** bar (`Inherited` recovery variant), replaying committed `CommandIdToken`s once and restoring unrun drafts strictly as unapplied drafts (`R-47`, `C-4`) `[P19-NEON-0237, P19-NEON-0242]`.

---

## 13.4 Per-Sheet View Preferences, Real Backing Objects, and Save Disclosures (`CL-13-04`–`CL-13-06`, `D-86`, `D-87`)

See [Pro 21 W04 — Budget: a view for each sheet](http://127.0.0.1:8720/p21/workbook.html#W04) and [Pro 21 O02 — File: save with the views](http://127.0.0.1:8720/p21/workbook.html#O02).

1. **Preferred & Supported Views Without Tab-Click Whiplash (`CL-13-04`, `D-86`):** `<prefs>` records a workbook-level `preferred` view, the `supported` view set, and optional per-sheet `preferred` hints (`W04`: `Inputs -> App`, `Monthly -> Classic`, `Summary -> Notebook`) `[FRAMING-0014, P21-0093]`. Every view still opens every workbook (`FRAMING-0014`), Classic is **never** hidden or locked out (`P21-0094`), and explicit reader choices override file defaults. Crucially (`D-86`), **clicking sheet tabs during authoring keeps the active view pinned** so analysts never suffer spatial whiplash: each sheet tab displays a subtle micro-glyph for its preferred view alongside a one-click Bridge suggestion chip (`Designed for Notebook · [Switch]`), while automatic view transitions occur only once on initial workbook open or when clicking an explicit Guided/App navigation link `[P19-FABLE-0221, P19-NEON-0103, P19-NEON-0114, D-86]`.
2. **Real Excel Backing Cells, Charts, and Serial Dates (`CL-13-05`):** Every computing Notebook cell, Model map formula port, and App input/output maps to a real cell or visible defined name on an ordinary Excel sheet (`Analysis` or a shared model sheet, with explicit scalar-row + array-column-lane spill management `A6`), Notebook charts save as genuine OOXML chart objects bound to dynamic `INDEX`/`ROWS` workbook names (`Chart.Region`, `Chart.Amount` over `rankedOrders`), and dates save as true Excel serial numbers formatted with number formats—never ISO prototype strings or DNA-only chart functions `[FRAMING-0151, P21-0021, P21-0034]`. Saving a map- or notebook-authored workbook and pressing `F9` in Excel produces bit-identical values `[P19-FABLE-0083]`.
3. **Save Disclosures, Stripped-Part Fallback, and Certified Save Routes (`CL-13-06`, `D-87`):**
   - **Save disclosures (`O02`, `R-105`):** Saving to `.xlsx`/`.xlsm` lists both model and view parts kept; if a formula calls an `.xll` add-in (`Seasonal.xll` / `DiscountFunctions.xll`), a non-blocking Strip disclosure notes **`◊ 1 formula uses Seasonal.xll · Excel shows #NAME? where not installed`** `[P19-FABLE-0104, P19-FABLE-0522]`. Exporting to `.csv`, `.pdf`, or legacy formats discloses upfront that `urn:dnacalc:views:v1` (or multi-sheet formulas) will not travel and offers **`Export .dnaskin.json sidecar`** (`O02`).
   - **Stripped-part resilience (`Push-back #4`, `D-87`, `R-62`):** Because Excel's *Document Inspector -> Remove Custom XML Data*, privacy scrubbers, or third-party converters can strip CustomXML parts, DNA Calc never promises blanket retention on untested editors `[P19-GPT-0091]`. When a workbook has no `urn:dnacalc:views:v1` part, it **opens in Classic by default** (showing a quiet notice + `[Restore from .dnaskin.json]` if local history recognises the file had views) while Inspector, Notebook, and Model map synthesize clean live views on demand from `Structure` (`D-87`) `[P19-GPT-0292, D-87]`.
   - **Honest verification boundary (`dc-1nbp.22`):** Pro 21 validated JSON serialization and mock round-trips only, not native `.xlsx`/`.xlsm` writers or desktop Excel preservation `[P21-0125]`. Certifying byte-level CustomXML, unknown-part, and `vbaProject.bin` round-trip across pinned Windows desktop Excel and Excel for the Web save routes is owned by open bead **`dc-1nbp.22`** (`RUN-ON-WINDOWS.txt`) `[P19-GPT-0091, P19-GPT-0098, P21-0125]`.

---

## 13.5 Preserved Native Workbook Objects and Cell-Level Metadata (`CL-13-08`, `CL-13-09`)

`calc-doc` and `calc-host` preserve and expose full round-trip fidelity across all native Excel structures so views read and edit real Excel metadata rather than parallel copies:

- **Structured workbook objects (`CL-13-08`):** `ListObject` Tables (name, range, typed columns, totals row, active filter/sort state, and structural growth notifications `C-13`, `C-23`), `PivotTable`s (source range, axis fields, cached values, last-refresh timestamp, and post-edit source-staleness flag `C-24`), 1D/2D `{=TABLE(r,c)}` Data Tables and native Excel `Scenario Manager` scenarios (changing cells, values, apply/create, and sheetless summary preview `C-25`), Form Controls (`ScrollBar`/`CheckBox` linked cell, min/max/step, writing identically to user clicks `C-26`), Charts/Shapes/Pictures (names, source series, and host `calc-chart-render` SVG/PNG renderings `C-32`), and Print Setup + exact Excel pagination (`PrintPlan`, `C-33`) `[P19-CARBON-0219, P19-FABLE-0547, P19-FABLE-0548, P19-FABLE-0549, P19-FABLE-0550, P19-FABLE-0556, P19-FABLE-0557]`.
- **Cell-level rules and visibility (`CL-13-09`):** Data Validation rules, prompts, Stop/Warning/Information alerts, and candidate-value pre-testing (`C-27`); Conditional Formatting rules in precedence order with per-cell fired-rule attribution (`C-28`); Number Formats rendered identically to Excel including conditional colour sections and locale while keeping raw stored values distinct from formatted text (`C-29`); cell `Locked`/`Hidden` flags plus sheet and workbook protection permissions (`C-30`); sheet visibility (`Visible`, `Hidden`, `xlSheetVeryHidden`) readable by Inspector without mutating file state (`C-31`); and legacy Notes plus threaded Comments CRUD (`C-34`) `[P19-FABLE-0551, P19-FABLE-0552, P19-FABLE-0553, P19-FABLE-0554, P19-FABLE-0555, P19-FABLE-0558]`.
