# Harvest 16: the interfaces between calc-engine, calc-host and calc-doc / calc-doc-model

Reader: Opus, 2026-11-12. Answer: `pro/16-engine-host-doc-interface-answer.md` (792 lines). Prompt:
`pro/16-engine-host-doc-interface-prompt.md` (1,427 lines). Both read in full.

Pins: DnaCalc 3b5aa86b (2026-11-12 09:01); CalcVBA 8115aa78 (read-only checkout). Every DnaCalc `file:line` below is at
3b5aa86b. Master was eccea1ab when I read it. `git diff --stat 3b5aa86b HEAD` over `crates/calc-host`, `calc-doc`,
`calc-doc-model`, `calc-engine`, `calc-fml/src`, `apps/dna-calc*` and `platform` touches only AutoFilter table visibility
(`calc-doc/src/load/filter.rs`, `load/mod.rs:2028-2040`), grid images and cell-entry replacement. None of it changes a
disposition here. The one line that moved: `apps/dna-calc/src/controller.rs:4239` is 4243 at HEAD.

How the checks were done. I read the code the answer cites, then the code behind each claim: the host command route
(`calc-host/src/command.rs`), engine prepare/commit (`calc-engine/src/edit.rs`, `state.rs`, `recalc.rs`, `external.rs`),
the reference tracker (`calc-fml/src/reference/tracked.rs`, `transform.rs`), history, the host save and the calc-doc
writer (`calc-host/src/save.rs`, `calc-doc/src/save/{mod,author,sheets,structural,parts}.rs`), the worker and skin save
route (`apps/dna-calc-worker/src/{server,workbook}.rs`, `apps/dna-calc/src/controller.rs`), and the tests named below. I
grepped the pinned tree, extracted with `git archive`, in `scratchpad/harvest-16/pin`. In CalcVBA I read
`crates/calcvba-office/src/storage.rs`. Bead checks: `.beads/issues.jsonl` read with Python. I did not run cargo, any test,
Excel or any `br` write. "Confirmed" below means confirmed by reading code, not by running it.

What the answer is. It read only the prompt (line 5). Its code claims are claims about the prompt's excerpts. Most are
careful and labelled [INFERENCE]. Several of its open P1 risks are already closed in code it did not see. It missed two
real persistence defects that the excerpts did not show.

---

## 1. Findings that change how the answer should be used

1. **Four of the answer's seven P1 risks are already handled in code.** Use them as regression ideas, not open defects.
   - *Late async results* (P1, row 1). `AsyncReturns::deliver` (`calc-engine/src/external.rs:82-127`) and
     `RecalcWork::accept_async` (`calc-engine/src/recalc.rs:6528-6544`) accept a return only if the work's revision equals
     both the model and engine revision, the generation matches, the workbook, formula generation and anchor match,
     and the provider resolves. Any commit advances the revision, so every old return is `Revoked`. A test covers
     edits, a cut, undo, provider change, cancel and reload (`calc-host/tests/round_trip.rs:8827-8880`). Row insertion,
     sheet insertion and table rebuild are not among its cases (bead 16.e).
   - *Failure neutrality* (P1, row 2). Engine prepare refuses before it reserves anything; a dropped
     `PreparedEngineChange` releases its node IDs (`calc-engine/src/edit.rs:181-193, 219-220`). The one host refusal
     after engine prepare, `FormulaBudget` (`command.rs:984`), is tested to leave the page unchanged
     (`calc-host/tests/acceptance.rs:484-500`). After `commit_edit` (`command.rs:1021`) nothing can fail: `rebuilt` is
     infallible (`calc-engine/src/state.rs:1198`).
   - *Source stability* (P1, row 6). The `PackageRead` contract requires stable bytes (`calc-doc/src/source.rs:58-61`),
     and `write` checks every copied entry's CRC-32 against the open-time value before the first byte, giving
     `WriteError::SourceChanged` with an empty sink (`calc-doc/CONTRACT.md:79-81`). Every product source is an
     in-memory `MemorySource` (`apps/dna-calc-desktop/src/engine.rs:216`).
   - *Inconsistent transform context* (P2, row 8). Wrong. `host.references` holds single-sheet `GridArea`s. For them
     `RemoveSheet` reads only the sheet id; `order` is used only when rewriting formula text
     (`calc-fml/src/reference/transform.rs:209-212, 290-294` versus `:516-517`; tracker `tracked.rs:144-161`). The empty
     order at `command.rs:1031-1034` is harmless.
2. **New defect: no save writes Excel's repairs.** A loaded duplicate `sheetId` gets a new id in the model
   (`LoadRepair::DuplicateSheetId`), but the writer never writes it. The `<sheets>` list is rebuilt only when the sheet
   list changed (`calc-doc/src/save/author.rs:381, 1037-1046`). When it is rebuilt, each existing `<sheet>` tag is reused,
   with only `state` and `name` rewritten (`calc-doc/src/save/sheets.rs:120-127`). An untouched save copies the bytes on
   purpose (`calc-doc/tests/reader_tolerance.rs:239-243`). The edited-save test reloads only in DNA Calc, which silently
   repairs again (`:244-255`). Excel's automation `Workbooks.Open` *refuses* duplicate ids and dangling `r:id`s
   (0x800A03EC, `crates/calc-doc/tests/fixtures/reader_tolerance/excel-answers.md:33-34`). So every DNA Calc save of such
   a file is a file Excel's object model cannot open. Excel's own saved ids are already recorded (same file, line 34),
   so the fix needs no new experiment for that case (bead 16.b).
3. **New defect: edits the save will refuse are accepted.** The host refuses sheet rename/remove broadly at edit time
   (`command.rs:907-925`). It does not check axis edits against preserved chart/dialog/partless sheets or macro sheets.
   The save then refuses with `PreservedSheetChanged` or `MacroSheetChanged` (`author.rs:389-409`). Example: in a book
   with a chart sheet, insert a row on a worksheet: the edit commits, and Save fails. The only way out is Undo. The
   precedent for an edit-time check already exists: `LoadedWorkbook::check_layout_save` (`calc-doc/src/load/mod.rs:1107-1137`),
   called at `command.rs:500`. Extend it; the answer's `PersistenceImpact` type is not needed (bead 16.a).
4. **New parity gap: the app never saves with CalculateBeforeSave = true, and it rewrites calcOnSave.** The skin sends
   `calculate_before_save: false` on every ordinary save (`apps/dna-calc/src/controller.rs:4239`). The loader never reads
   `calcOnSave` (no read anywhere in `crates/calc-doc/src`). A Manual-mode save then writes `calcOnSave="0"`
   (`calc-doc/src/save/parts.rs:491-500`, via `calc-host/src/save.rs:201-208`). Excel's default is CalculateBeforeSave
   true. So a Manual workbook saved by DNA Calc keeps stale caches where Excel would have recalculated, and it changes the
   persisted setting. The CalculateBeforeSave = true route exists and is tested
   (`calc-host/tests/round_trip.rs:8370-8410`); the skin just never selects it (bead 16.c, experiment E13).
5. **Undo of a structural edit, and any table change, drop every displayed value.** Both rebuild the engine "every node
   pending, nothing published": no calculated results and no file caches (`calc-engine/src/state.rs:1193-1198`; callers
   `command.rs:1023-1025` for `SetTables`, `command.rs:1219-1224` for undo restore; undo of axis and sheet edits always
   restores, `calc-host/src/history.rs:149-173`). The forward structural edit keeps moved results
   (`calc-engine/src/structural.rs:7-10`), so the asymmetry is ours. In Manual mode the grid then shows stale-empty
   cells (`calc-host/src/project.rs:337-339`) until F9. A Manual Save(false) then keeps no caches. In Automatic mode every
   formula, including every UDF, is evaluated again. `RECALC_CONTRACT.md:642-645` records the rebuild as today's design;
   the Excel side is unmeasured (bead 16.d, experiment E12).
6. **The app cannot reach two of the answer's four fidelity promises.** Every app save uses
   `SaveFormat::Spreadsheet(format)` (`apps/dna-calc-worker/src/server.rs:994-998`, `workbook.rs:427-432`). That always
   rewrites and recompresses `[Content_Types].xml` (`calc-doc/src/save/mod.rs:941-983`; `Written::replacing` deflates
   again, `:643-663`). The skin also sends window views on every save (`controller.rs:4223-4241`). So no app save is
   ZIP-identical, and the original-XLSB route (`save/mod.rs:986-998`) is unreachable from the app: no `SaveFormat::Xlsb`
   anywhere in `apps/`. The byte-identity tests use calc-doc's default policy (`reader_tolerance.rs:90`,
   `CONTRACT.md:83-85`). Claims about "untouched save" must name the route.
7. **Experiments E1, E3 and E5 are mostly measured already.** P06/P07 settled Save/SaveAs/SaveCopyAs, BeforeSave order,
   dirty peers and disabled sheets (`docs/architecture/RECALC_CONTRACT.md:690-703, 837-859`). dc-09v and dc-jhn measured
   cut/copy/axis rules (`calc-fml/src/reference/transform.rs:78-82`; `calc-host/tests/round_trip.rs:3571`). P09 measured
   Formula versus Formula2 across ten ingress paths (`RECALC_CONTRACT.md:1550`). Run only the remaining branches
   (section 5).

---

## 2. Disposition of every section

| Section | Content | Disposition |
|---|---|---|
| 1 intro | Keep the architecture; contracts hold across routes | no action: agrees with the code |
| 1 What to keep: loaded vs current | `HostState.loaded` / `model` / `engine` roles | no action: already true (`calc-host/src/state.rs:102-117`) |
| 1 What to keep: prepared edits | Stale base, document then engine prepare, commit order | no action: confirmed (`command.rs:338-344, 896-942, 1021-1042`) |
| 1 What to keep: pinned saves | `SavePin`, `PreparedSave` | no action: confirmed (`calc-host/src/save.rs:134-160`) |
| 1 What to keep: rewriter | `ReferenceRewriter` over calc-fml | no action: confirmed (`calc-engine/src/structural.rs:1-82`) |
| 1 What to keep: refusals | Macro-sheet and preserved-sheet refusals, MacroLoss | no action, but see finding 3: the edit-time side is missing (**16.a**) |
| 1 End-to-end table | Six routes, established vs not | no action as a table. Rows 4 and 5 are answered: CalculateBeforeSave orchestration exists (`save_calculation.rs`), repairs reach only the corpus harness (`tools/xtask-harness/src/classify.rs:967-1001`), not the skin (**16.b** delivers them). Row 6 is answer 13/14/18 territory |
| 1 Reconcile: atomic structural edits | October async trace is a test, not a defect | **16.e** (add the axis/sheet/table cases to the existing late-return test) |
| 1 Reconcile: chart/dialog references | dc-mndh refusal; embedded charts open | no action: embedded charts are covered. Axis edits rewrite every chart part's `c:f` (`calc-doc/src/save/structural.rs:1904-1919`). Rename/remove is refused when any chart exists (`command.rs:913-918`) |
| 1 Reconcile: repair reporting | Reaches HostState; UI open | **16.b** (delivery to the skin with the save fix) |
| 1 Reconcile: XLSB | No authored writer; original preserved on empty change set | **dc-snz.14** (writer). Add to it: the app never selects `SaveFormat::Xlsb` (finding 6) |
| 1 Reconcile: XLSB conversion loss | Declared limits | no action: documented (`calc-doc/src/xlsb/mod.rs:16-21`) |
| 1 Measurement populations | Four populations kept apart | no action: matches the prompt |
| 1 Verdict | Risk is disagreement between edit, engine, references and package | no action as a statement. The concrete disagreements are findings 2-5 |
| 2 P1 late-result | Address reuse after structural edit | no action as a defect (finding 1). **16.e** test; **16.h** E9 for Excel's transfer rule |
| 2 P1 failure neutrality | Mutable engine prepare, table rebuild after commit | no action: handled (finding 1) |
| 2 P1 stale array/spill caches | Arrays excluded from PreservePermittedCaches | confirmed limitation (`save.rs:82, 321-323`). **16.g** after E2 |
| 2 P1 normalized model vs original bytes | Repairs not written back | **16.b**: demonstrated by reading (finding 2) |
| 2 P1 edit/save disagreement | Accepted edit later refused | **16.a**: demonstrated by reading (finding 3) |
| 2 P1 source stability | Lazy reads of a changing source | no action: handled (finding 1) |
| 2 P1 failed physical save | Partial sink, destination truncation | no action in calc-host: the sink is a `Vec<u8>` (`workbook.rs:984-993`). The file is a WebView2 download, and the skin keeps `download_unconfirmed` (`controller.rs:4230-4248, 4269-4300`). Destination replacement is the download manager's |
| 2 P2 transform context | Empty sheet order for host references | no action: wrong (finding 1) |
| 2 P2 identity misuse | Equality excludes lineage | no action: `command_scope` (`state.rs:110`, `paste.rs:63`) and `SaveRefusal::ForeignModel` by lineage (`author.rs:375-377`), tested on twin loads (`calc-doc/tests/acceptance.rs:8257-8276`) |
| 2 P2 repeated structural preparation | Axis batch applied twice | deferred (perf after publish; batches of more than one axis edit are rare) |
| 2 P2 broad recalculation | All nodes pending after structural edits; table rebuild | **16.d** (with E12); the forward-edit cost: deferred to perf |
| 2 P2 XLSB fidelity/routing | Same-format save conditions | **dc-snz.14**, plus finding 6 |
| 2 P2 VBA source/signatures | Preserved bytes vs write-back | no action now. Facts: CalcVBA writes projects and refuses signed ones (`calcvba-office/src/storage.rs:126-145, 497-510`). OneCalc uses it (`apps/dna-onecalc/src/session.rs:831`). DNA Calc's `LoadVbaModule` "Source is not persisted here" (`apps/dna-calc-model/src/projection.rs:911`). DNA Calc write-back: deferred to **dc-snz** |
| 2 qualification 1 | Sheet ID reservations need not roll back | no action: agrees (`command.rs:927-929`; keyed by `SheetKey`, so a retry reuses it) |
| 2 qualification 2 | dc-mndh stays closed | no action: agrees |
| 3.1 State ownership table | Owners per state | no action: it describes today's owners. One correction: authored VBA source has no owner in DNA Calc (see 2 P2 VBA) |
| 3.2 Identities and invariants | Six invariants | no action: 1, 2, 5, 6 hold in code (finding 1). 4 is **16.a**. 3 is met by the one route `dispatch_slice` (`command.rs:330`, only caller `session.rs:1393`) |
| 3.3 AuthoredDelta / PreparedEdit extension | One shared change description | deferred: no disagreement between doc and engine was found. Both take the one `OpBatch`, and the engine stages every node as moved. Revisit if 16.a or answer 15 shows one |
| 3.4 Operation semantics table | Per-operation treatment | no action: matches the code's split (copy `relocate_formula`, cut `move_references`, structural rewrite). The table/name rows go to E4 |
| 3.5 Commit and failure behaviour | Outcome table | no action: holds (finding 1). "Save writing fails" is the download route (2 P1 failed save) |
| 3.6 Undo, provenance, history | Three histories; eviction must not lose source transforms | no action: axis edits live in `host.axis_edits` (`command.rs:1051`), not in the undo stack, so eviction (`history.rs:427-433`) loses nothing. Handle revival on undo is answer 14 (E10) |
| 3.7 Save sequence | Nine steps | no action: matches `ExcelSaveContext`/BeforeSave (`save.rs:51-72`) and P06 (`RECALC_CONTRACT.md:837-843`) |
| 3.7 Cache policy table | Seven situations | Manual false: done (P06). True: **16.c**. Arrays: **16.g**. The rest: no action, already true (`load/mod.rs:1157-1167`; reopen imports as unverified) |
| 3.7 Four fidelity promises | Byte, entry, semantic, repair-free | no action as a doc change (light process). Finding 6 says which promise each route can make. E6/E7 test them |
| 3.7 Can an accepted edit prove unsavable | Surface at admission | **16.a** (reuse calc-doc's refusals; no `PersistenceImpact` type) |
| 3.8 Repairs and opaque features | Normalize on authored save vs preserve original | **16.b** for normalizing. A "preserve original with defects" mode: no action: against bit-match, since Excel's own save writes the repaired workbook (`excel-answers.md:33-34`). Opaque-sheet bullets: no action, already true |
| 3.9 VBA, native, wasm, skin | Policy dimensions, owner thread, OneCalc values-only | no action: matches owner rules (macro levels, CalcVBA Safe/Full, OneCalc values-only). Process isolation: **dc-6332.15**. Skin repair delivery: **16.b** |
| 3.10 Shared contracts table | Owners across five reviews | no action: reference for harvests 14, 15, 17, 18 |
| 3.11 Large workbooks | Costs to measure first | deferred (perf after publish). Notes: `ModelRoot::pin` is O(sheets) `Arc` bumps (`calc-doc-model/src/cells.rs:327-329`), not a page copy. Product sources hold the whole file in memory, and an XLSB holds the original plus the converted package (`calc-doc/src/lib.rs:163-167`) |
| 4 S0 route evidence | Instrumented round trips | no action: the round-trip and P06/P07 tests already carry pins, receipts and tokens. No new evidence apparatus (owner rule) |
| 4 S1 publication invariants | Failure injection | no action: done (finding 1) |
| 4 S2 shared transformations | One semantic path per operation | deferred with 3.3. The sheet-order item is wrong (finding 1) |
| 4 S3 late work and undo | Async across structural edits | **16.e**, **16.d** |
| 4 S4 save guarantees | Repairs, arrays, eligibility, source, destination | **16.a**, **16.b**, **16.c**, **16.g**. Source and destination: no action (finding 1; 2 P1 failed save) |
| 4 S5 adapters and presentation | JS/VBA/skin through one route | repair delivery **16.b**; JS → answer 14; VBA callbacks → answer 18 / dc-hsa5.12 |
| 4 S6 optimize | After attribution | deferred (perf after publish) |
| 4 Correctness list | Eight mandatory properties | no action: they restate AGENTS.md honesty rules |
| 4 Performance budgets | 16 ms native / 50 ms wasm | deferred (perf after publish) |
| 5 Common protocol | Fresh processes, seeds, capture fields | no action: our Excel session host already does this (`excel-answers.md:11-26`) |
| 5 E1 | Save scope, BeforeSave, SaveCopyAs | mostly measured (P06/P07). Remainder in **16.h** |
| 5 E2 | Stale array caches | **16.h**, then **16.g** |
| 5 E3 | Copy, cut, axis, partial range | mostly measured (dc-09v, dc-jhn). Two rows in **16.h** |
| 5 E4 | 3-D spans, scoped names, relative names, tables | **16.h** |
| 5 E5 | Formula dialect, shared holes, arrays | dialect measured (P09). Shared hole and spill obstruction in **16.h** |
| 5 E6 | Opaque references, fidelity | **16.h**; feeds **16.f** |
| 5 E7 | Repairs and untouched save | **16.h**; decides the untouched-save part of **16.b** |
| 5 E8 | XLSB/XLSM, VBA source, signatures | deferred (waits on dc-snz.14 and a DNA Calc source write-back route) |
| 5 E9 | Late async with structural edits and save | **16.h** (needs the async XLL fixture) |
| 5 E10 | Office.js handles | answer 14 harvest; deferred here |
| 5 E11 | Worksheet restrictions, threads | answer 18 harvest; deferred here |
| 6.1 | Baseline and current root are not duplicates | no action: agrees |
| 6.2 | Retire stale critiques | no action: agrees |
| 6.3 | Remove repeated structural application | deferred (perf) |
| 6.4 | One accepted change description in adapters | deferred with 3.3; today all edits pass `dispatch_slice` |
| 6.5 | No address-only async authority | no action: already true (finding 1) |
| 6.6 | DropStaleCaches not Excel-equivalent | no action: `CurrentCalculatedOnly` is used outside Manual XML; P06 chose the Manual profile |
| 6.7 | Do not conflate fidelity promises | no action (finding 6 records which route makes which) |
| 6.8 | XLSB loss stays visible | **dc-snz.14** |
| 6.9 | Preserved sheets are not modelled surfaces | no action: already true |
| 6.10 | VBA bytes do not prove write-back or signatures | no action: CalcVBA refuses signed write-back |
| 6.11 | No second evaluator/graph/model | no action: agrees with AGENTS.md |
| 6.12 | Defer storage rewrites | deferred (perf) |
| 6.13 | Preserve compatibility evidence | no action: agrees |

---

## 3. Claims checked against the code

| # | Claim (answer section) | Verdict | Evidence at 3b5aa86b |
|---|---|---|---|
| 1 | `loaded` is the load, `model` the current root, `engine` at that revision (1) | confirmed | `calc-host/src/state.rs:113-117` |
| 2 | Stale base refused; document then engine prepared; engine committed, root published, then `Committed` (1) | confirmed | `command.rs:338-344, 896-942, 1021-1042` |
| 3 | `PreparedSave` owns its plan; writing consumes it (1) | confirmed | `save.rs:145-159`; edits after the pin do not reach the image (`round_trip.rs:8393-8402`) |
| 4 | Structural route moves nodes and results, keeps IDs, leaves all unbound and pending (1) | confirmed | `structural.rs:7-11`; `edit.rs:239-256` invalidates every node |
| 5 | Macro-sheet structural and preserved-sheet rename/removal refused at save (1) | confirmed | `author.rs:389-409` |
| 6 | Conversion to a macro-free format refuses macro loss (1) | confirmed | `save/mod.rs:954-963` |
| 7 | XLSB original preserved only with an empty change set; cache or view changes can break that (1) | partly | Confirmed at `save/mod.rs:986-998`. Caches alone do not break it: a save of the loaded revision copies the package "values or not" (`load/mod.rs:1166-1167`). Views do. The app never asks for it (finding 6) |
| 8 | XLSB conversion has declared limits (1) | confirmed | `xlsb/mod.rs:16-21` |
| 9 | Repairs reach `HostState::open_report` (1) | confirmed | `state.rs:418-426`; consumers: the corpus harness only (`tools/xtask-harness/src/classify.rs:967-1001`) |
| 10 | Late async result could reach a moved or reused formula (2) | wrong as a risk | `recalc.rs:6528-6544`, `external.rs:82-127` (finding 1) |
| 11 | Engine prepare mutates state; failure could leave model and engine inconsistent (2) | wrong as a risk | `edit.rs:181-193, 219-220`; `acceptance.rs:484-500` |
| 12 | Table changes rebuild after the commit (2) | confirmed | `command.rs:1023-1025`; consequence in finding 5 |
| 13 | PreservePermittedCaches excludes arrays and current spills; dirty formulas then skip (2) | confirmed | `save.rs:321-345`; `fullCalcOnLoad` when an array lacks a value (`load/mod.rs:1163-1166`) |
| 14 | `Edited.base` is the normalized model (2) | confirmed | `save/mod.rs:89-90`; repairs never written back (finding 2) |
| 15 | An edit may be accepted and later refused by persistence (2) | confirmed | Axis edits: `author.rs:389-409`, no edit-time check (finding 3) |
| 16 | Source read lazily during save; external changes could mix versions (2) | wrong as a risk | `source.rs:58-61`; CRC check before any byte (`CONTRACT.md:79-81`) |
| 17 | The sink is append-only; a partial write could damage a destination (2) | partly | Append-only: `source.rs:78-81`. The product sink is a `Vec<u8>` and the file a WebView2 download (`workbook.rs:984-993`, `app.rs:727-729`) |
| 18 | Host references receive an empty sheet order; a 3-D handle could diverge (2) | wrong | `transform.rs:209-212, 290-294`; tracker areas are single-sheet (`tracked.rs:153`) |
| 19 | Equality excludes lineage; `command_scope` should reject the wrong instance (2) | confirmed | `root.rs:261-268`; `paste.rs:63`; `author.rs:375-377` |
| 20 | Each axis operation applied to temporary roots, then the batch again (2) | confirmed | `calc-doc/src/edit.rs:262-281` |
| 21 | Sheet identity issued before preparation completes (2) | confirmed | `command.rs:927-929` |
| 22 | History restores a popped step whose undo did not commit (3.6) | confirmed | `history.rs:443-451` |
| 23 | "Undo never restores a deleted scope" is a code policy, not measured (3.6) | partly | Code policy (`history.rs:77`, `tracked.rs:2`), recorded as the contract (`RECALC_CONTRACT.md:794-796`). Excel side for COM/JS handles: E10 (answer 14) |
| 24 | Snapshot carries base, model, values, spills, structural edits, cache state, pivots, custom XML (3.1) | confirmed | `save/mod.rs:82-106` |
| 25 | Save scans formula cells and builds maps (3.11) | confirmed | `save.rs:315-316` |
| 26 | `ModelRoot::pin()` clones outer collections (3.11) | confirmed, cost small | `root.rs:276-284`; `SheetPages` is one `Arc` per sheet (`cells.rs:327-329`) |
| 27 | XLSB keeps the converted XML package and the original (3.11) | confirmed | `lib.rs:163-170` |
| 28 | History retains roots and sometimes loaded workbooks (3.11) | confirmed | `history.rs:54-72` |
| 29 | Coverage of embedded charts on ordinary sheets is open (1, 3.8) | wrong | Axis: `save/structural.rs:1904-1919` rewrites all chart `c:f`. Rename/remove: refused when any chart exists (`command.rs:913-918`) |
| 30 | A runtime VBA source edit is not a persisted project edit (3.9) | confirmed for DNA Calc | `projection.rs:911`; OneCalc does write back (`dna-onecalc/src/session.rs:831`) |
| 31 | Signature bytes retained are not proof of validity (3.9) | confirmed by refusal | CalcVBA refuses signed write-back (`storage.rs:137-145, 497-510`) |
| 32 | CalculateBeforeSave orchestration not established (1 table) | wrong for calc-host, right for the app | Host: `save_calculation.rs`, P06/P07 tests (`round_trip.rs:8085-8410`). App: always false (finding 4) |
| 33 | Untouched save can be byte-identical (3.7, E7) | partly | calc-doc default policy: yes (`reader_tolerance.rs:90`, `CONTRACT.md:83-85`; gap bytes are dropped and counted). App route: no (finding 6) |
| 34 | `Workbook.Saved` is not a per-cell dirty flag; `Range.Dirty` changes state (5) | confirmed (Excel object model documentation) | Not testable here; matches the public VBA reference |
| 35 | UI open and automation load differ on repairs (3.8) | confirmed [MEASURED-EXCEL] | `excel-answers.md:30-37` |
| 36 | SaveCopyAs skips BeforeSave (E1 hypothesis) | already measured | P06, `RECALC_CONTRACT.md:690-691, 838-840` |
| 37 | Formula vs Formula2 is a semantic distinction (E5 H1) | already measured | P09, `RECALC_CONTRACT.md:1550` |
| 38 | Cutting part of a referenced range leaves the range as written (E3 H2, value 5) | already measured in our rules | `transform.rs:284-290`: an area moves only when both corners are inside the cut; dc-09v measured `move_site` (`transform.rs:78-82`) |
| 39 | 3-D span membership follows sheet order after a move (E4 H1) | not checkable here | Code rebinds every node after any sheet move (`edit.rs:800-814`), so it follows whatever binding does. Excel: E4 |

---

## 4. Experiments

All runs need the Windows Excel desktop: they save, reopen, use events, COM or UI undo. None is a pure formula
evaluation for the answer service. "Runs" counts workbook runs (open, act, save, reopen). Hypotheses are predictions,
not measurements. Each run records the saved package before reopening, the calcPr attributes, `<v>` values and types,
UDF call counts and the dialog titles.

| Exp | Question | Inputs and steps | Outcome per hypothesis | Runs |
|---|---|---|---|---|
| E1 rest | Does opening order change Save(true)'s domain? Does Save(true) wait for a pending async? | P06 seeds T and P with `Audit`; open P first, then T; Save, SaveAs, SaveCopyAs × CBS false/true. Plus Save(true) with an async UDF pending | Order-independent domain: same caches as P06. Order-dependent: peer stays 10. Async: H1 waits; H2 stale cache kept; H3 save cancels the calc (as P07 saw for false) | 6 + 3 |
| E2 | Does Manual Save(false) keep stale array caches? | Legacy CSE `{=A1*{1;2}}` and dynamic `=A1*{1;2}` with A1=2, cached 2, 4. Manual, CBS false, set A1=3, save without F9, reopen Manual and Automatic. Add an array UDF that counts calls | Keep: caches 2, 4 with `ca`/`calcCompleted=0`, no reopen call. Drop: no `<v>` or `fullCalcOnLoad=1`, reopen calls the UDF. Anchor and members differ: record both | 4 + 2 |
| E3 rest | Do cross-sheet dependents follow a cut? | `Data!B1=$A$1+A2`, `Watch!A1=Data!B1`; cut B1 to C2. Second book: SUM(A1:A3), cut A2 to C2 | Follow: Watch reads `Data!C2`, 30. Fixed: Watch reads empty B1. Partial range: 5 (our rule) or 7 | 2 |
| E4 | 3-D order, scoped name fallback, relative names, table rename and growth | The answer's seed (First/Middle/Last/Watch, Rate × 2, RelLeft, table T) with its five branches; save and reopen each | As the answer states: 101 vs 111; 3 kept; 2 on fallback vs error; 9 vs error vs other; 8 vs 3 | 5 + 5 reopen |
| E5 rest | Shared-formula hole; spill grows into an obstruction | Package fixture with a shared group of three and the middle member set to 999; spill `=SEQUENCE(A1)` with an obstruction just past its extent, then A1 grows | Hole: Excel opens with no repair and keeps 999, vs repair. Obstruction: `#SPILL!`, obstruction kept | 2 |
| E6 | Which preserved references does Excel rewrite on rename, remove, row insert and reorder? | Four seeds: embedded chart, chart sheet, dialog sheet with a control link, macro sheet; each refers to `Data!A1:A2`. Branches: rename, remove, insert row 1, reorder, hide. Read series `c:f`, control link, macro formula text | H2 (rewrite) is expected for charts on rename and insert, `#REF!` on remove. Reorder changes nothing (H4 false). The result decides which refusals **16.f** can replace | 4 × 5 = 20, + 20 reopen |
| E7 | What does Excel save after a repair, and does a DNA Calc save reopen without one? | The six reader-tolerance fixtures. (a) Excel ordinary Save after UI open, not SaveCopyAs. (b) Open DNA Calc's untouched and edited saves through automation and File > Open | (a) Normalize: ids as SaveCopyAs wrote them (`excel-answers.md:34`); a repaired workbook might also force Save As. (b) Today, predicted from code: duplicate and dangling are still refused by automation and prompt in the UI (finding 2). After **16.b**: no prompt | 6 + 6×2×2 = 30 |
| E12 | Does Excel keep displayed values on Undo of a row insert or a table rename, and what does it recalculate? | Manual: 1,000 formulas, 10 of them a counting UDF; insert row 5 by UI, then Ctrl+Z; separately rename a table column used by 2 formulas, then Ctrl+Z. Read values before F9, then F9 and count calls. Repeat in Automatic | Keep: values shown and no calls before F9; F9 calls only the dirty ones. Rebuild-like: all 10 UDFs called. Ours today: blank values in Manual, all 10 called in Automatic (finding 5) | 4 |
| E13 | Where does CalculateBeforeSave come from, and what does Save write? | Three Manual books: `calcOnSave` absent, `"1"`, `"0"`. Open each as the first book in a fresh process; read `Application.CalculateBeforeSave`; edit a precedent; Ctrl+S; inspect calcPr and caches. Then open a second book with another setting first | Documented behaviour: the first book opened sets the application value, and Save writes it back. Ours: always false and writes 0 (finding 4) | 3 + 2 |
| E9 | Late async across a row insert, an overwrite, undo and save | The answer's E9 with our XLL async fixture | H1 transfer, H2 restart, H3 defer. Ours: H2/H3; the old return is `Revoked` | 8 |
| E8, E10, E11 | — | — | deferred (section 6) | — |

Total for **16.h** as listed: about 130 desktop runs. E7, E12 and E13 are the cheapest and decide the beads to start
first.

---

## 5. Proposed beads

**16.a Refuse at edit time what the save would refuse.** Priority 1. Start first: yes (small, protects work, no
experiment needed).
- What: `dispatch_slice` accepts an axis edit in a workbook with a preserved chart, dialog or partless sheet, or with a
  macro sheet. The save then refuses (`calc-doc/src/save/author.rs:389-409`). Cell edits on a macro or partless sheet do
  the same (`dc_xuw_macro_sheet.rs:79-90`, `reader_tolerance.rs:111-122`).
- Where: generalize `LoadedWorkbook::check_layout_save` (`calc-doc/src/load/mod.rs:1107`) into one check that reads the
  same conditions as `author::plan`. Call it where `check_layout_save` is called (`calc-host/src/command.rs:500`). Return
  the same `SaveRefusal`, wrapped in `CommandError::LayoutSave`.
- Accept: a host test per case: the command is refused with the save's own refusal, revision and page unchanged, and an
  unrelated cell edit in the same book still saves.
- Depends on: nothing. Model: Sonnet.

**16.b Write Excel's repairs on save, and show them.** Priority 2. Start first: yes (the Excel answers are already
recorded).
- What: no save writes a `LoadRepair` (finding 2). Excel's saved repairs are measured: duplicate ids
  (`excel-answers.md:34`), dangling `r:id` becomes a sheet with its id and a part (`:33`), a formula inside an array is
  dropped (`:36`).
- Where: `calc-doc/src/save/sheets.rs:110-140` (write `entry.key()` as `sheetId` when the load repaired it);
  `author.rs:1037-1046` (rewrite the workbook part when the load holds repairs); a part for a dangling sheet; drop the
  `<f>` of a `FormulaInsideArray` cell. Skin: carry `HostState::open_report()` to the worker's open reply and show it in
  the status line.
- Accept: for each repaired fixture, the saved package loads with `repairs()` empty and the same model. The sheet ids
  equal `sheets-of-the-workbooks-excel-saved-after-opening.json`. The untouched-save assertion
  (`reader_tolerance.rs:240-242`) changes once E7(a) shows what Excel's ordinary Save writes. E7(b) confirms: no prompt
  in Excel.
- Cheap side check: whether dc-ljie's two "stage reopen" workbooks report repairs.
- Depends on: E7(a) for the untouched case only. Model: Sonnet.

**16.c CalculateBeforeSave from the workbook.** Priority 2. Start first: no (E13 first).
- What: the skin always sends `calculate_before_save: false` (`apps/dna-calc/src/controller.rs:4239`); the loader never
  reads `calcOnSave`; Manual saves write `calcOnSave="0"` (`calc-doc/src/save/parts.rs:491-500`).
- Where: read `calcOnSave` into `LoadedWorkbook` beside `calc_mode`. Expose it on the host. The skin's Save uses the
  session's value. The writer writes the session's value back.
- Accept: per E13's three books, the saved calcPr and caches equal Excel's. Save with true in Manual runs the existing
  calculated-save route (`round_trip.rs:8370-8410` shape) from the skin.
- Depends on: E13. Model: Sonnet; Opus to decide the application-level rule if E13 shows first-book-wins.

**16.d Keep published results through Undo restore and table rebuild.** Priority 2. Start first: no (E12 first).
- What: `EngineState::rebuilt` drops every published result (`calc-engine/src/state.rs:1193-1198`). Used for
  `SetTables` (`command.rs:1023-1025`) and for undo of axis and sheet edits (`command.rs:1219-1224`). Manual mode shows
  blanks; Automatic re-evaluates every UDF.
- Where: carry the prior results as stale displays across the rebuild, extending `rebuilt_with_held_caches`
  (`state.rs:1234`). Whether to re-seed only dependents follows E12.
- Accept: after Undo of a row insert in Manual mode, every formula cell projects its previous value as `Stale`, not
  empty. A Manual Save(false) keeps those caches. The Automatic UDF call count after Undo equals E12's Excel count.
- Depends on: E12. Model: Codex terra/sol (engine code); Opus review of the stale-result rule.

**16.e Late-return regression cases for structural edits.** Priority 3. Start first: no.
- What: extend `contract_p07_edits_moves_undo_reload_cancel_and_reopen_reject_late_results`
  (`calc-host/tests/round_trip.rs:8827`) with: insert a row above the async cell; insert a sheet; change a table; the
  answer's trace (insert row 5, put `=777` at the old address, return the old token, undo).
- Accept: each old return is `Rejected`; the revision is unchanged; `=777` keeps 777; no suffix code runs.
- Depends on: nothing. Model: Sonnet.

**16.f Sheet rename and remove with tables, charts and rules.** Priority 2. Start first: no.
- What: the host refuses rename/remove if the workbook has any table, chart, conditional rule or anchored layout block,
  anywhere (`command.rs:907-925`). Most real workbooks have one of these, so rename is refused for most of them. The
  axis route already rewrites chart `c:f` (`save/structural.rs:1904-1919`) and rule formulas.
- Where: a rename/remove pass that reuses `chart_pass` with `StructuralEdit::RenameSheet`/`RemoveSheet`, rule formulas
  through `ReferenceRewriter`, and table `sheet` ids. Keep refusing pivots, macro, dialog and partless sheets.
- Accept: rename and remove of a sheet named by a chart series, a rule and a table save and reopen as Excel's E6 package
  does (series text, `#REF!` on remove). Refusals stay typed where E6 shows a rewrite we do not model.
- Depends on: E6; links dc-971p. Model: Opus design, Codex implementation.

**16.g Array caches under Manual Save(false).** Priority 3. Start first: no.
- What: dirty arrays never keep caches, and a missing array value sets `fullCalcOnLoad` (`save.rs:82, 321-323`;
  `load/mod.rs:1163-1166`).
- Where: `calc-host/src/save.rs:315-347` and the spill writer.
- Accept: E2's saved packages reproduced byte-for-value (`<v>`, `t`, `ref`, flags).
- Depends on: E2. Model: Sonnet.

**16.h Excel experiments from answer 16.** Priority 2. Start first: yes, for E7, E12 and E13 only (about 40 runs; they
decide 16.b, 16.c and 16.d).
- What: section 4's list, run with the session host on the private desktop. Raw packages go to R2. A results note goes
  in `crates/calc-doc/tests/fixtures/` beside `reader_tolerance/excel-answers.md`, or in RECALC_CONTRACT for the save
  probes, extending what is there.
- Accept: every branch has a recorded package and observation. Each hypothesis is marked held or refuted.
- Model: Sonnet with the Excel session tools.

---

## 6. Deferred list

| Item | Waits on |
|---|---|
| 3.3 / 6.4 one `AuthoredDelta` for doc, engine, handles, save | A demonstrated disagreement beyond 16.a; answer 15's harvest (graph side) |
| 6.3 / 2 P2 remove the double structural application (`calc-doc/src/edit.rs:262-281`) | The perf phase after publish (owner direction 2026-10-27) |
| 3.11, S6, performance budgets, forward structural-edit rebind cost | The perf phase after publish |
| E8 XLSB/XLSM conversion, VBA source write-back in DNA Calc, signatures | dc-snz.14 (XLSB writer); a DNA Calc source write-back route under dc-snz (OneCalc's exists); a code-signing certificate for signed variants |
| E10 Office.js tracked ranges and batching | Answer 14's harvest (object model and JS) |
| E11 worksheet-function restrictions, reentrancy, thread IDs | Answer 18's harvest; dc-hsa5.12 |
| Process isolation (3.9) | dc-6332.15 |
| Embedded-model (data model) changed-row save (`save.rs:262-302` refusal, dc-8wp.20) | dc-8wp |

