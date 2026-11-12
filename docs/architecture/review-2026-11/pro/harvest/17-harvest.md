# Harvest 17: host commands, worker protocol and the skin IR (GPT-6 Pro answer)

Reader: Opus, 2026-11-12. Answer: `pro/17-host-to-skin-ir-answer.md` (769 lines). Prompt:
`pro/17-host-to-skin-ir-prompt.md` (1,510 lines). Both read in full.

Pins: DnaCalc `3b5aa86b` (2026-11-12 09:01); CalcVBA `8115aa78`. Every `file:line` below is at the DnaCalc pin unless
a commit is named. The answer makes no claim about CalcVBA code, so the CalcVBA checkout was not needed.

**Drift to master (eccea1ab).** `git diff --stat 3b5aa86b HEAD` over `apps/dna-calc-model`, `apps/dna-calc-protocol`,
`apps/dna-calc-worker`, `apps/dna-calc/src`, `apps/dna-calc-desktop`, `crates/calc-host/src/session.rs` and
`docs/architecture/ideas` shows 8 files. Seven are small (IMAGE cell kind, an edit-session `replacement` flag, a stale
IMAGE fallback) and change no disposition. One does: `docs/architecture/ideas/LINKED_BOXES_SKIN.md` gained an owner
direction section in 41e3181b (dc-lvlj, lines 50-62 at master). It is applied in section 2 and finding 5.

**How the checks were done.** Read AGENTS.md. Read the pinned code with `git show 3b5aa86b:<path>`: in full or in the
relevant parts for `apps/dna-calc-model/src/tile_mirror.rs`, `tiles.rs`, `render_plan.rs` (attach_semantics),
`runtime.rs`, `state.rs`; `apps/dna-calc-worker/src/publication.rs`, `server.rs` (subscribe, ack, reset, install,
collect, notebook, overhang, command admission, tests), `worker.rs`, `tree.rs`; `apps/dna-calc/src/controller.rs`
(receive, subscribe, installed), `port.rs`, `grid/mod.rs`, `grid/panes.rs`, `canvas.rs`, `skin.rs`, `app.rs`;
`apps/dna-calc/web/desktop-bridge.js`; `apps/dna-calc-desktop/src/app.rs`; `crates/calc-host/src/session.rs`
(snapshot, deliver_reset, deliver_changes), `crates/calc-host/src/envelope.rs`; `crates/calc-doc-model/src/layout.rs`;
`crates/calc-doc/tests/acceptance.rs` (cdoc-m2.1 header). Large files were copied to
`C:\Users\GOVERT~1\AppData\Local\Temp\claude\83b2dd6fa0a97a19\81032ca3-538f-4a32-9add-1dc271eeef42\scratchpad\harvest-17\`
for grep. Bead searches were Python reads of `.beads/issues.jsonl`. No cargo, no tests run, no Excel, no `br` writes.
Where I say a test exists, I read its body; I did not run it.

---

## Findings that change how the answer should be used

1. **Most of the answer's P1 "risks" are already handled in code, with tests.** The answer could see only excerpts, so it
   listed as open what the route already does:
   - Stale lifecycle. Page subscription ids never repeat across workbooks (`controller.rs:4064-4069`;
     `runtime.rs:420`, never reset in `state.rs:181-195`). Mirrors are cleared on install (`controller.rs:990`). The
     server drops queued tile events on install (`server.rs:1691-1693`). Old acks cannot release a new flight (test
     `server.rs:5626-5661`). Late events to a retired view do nothing (test `controller.rs:4794-4846`).
   - Composite-view tearing in the grid. The grid has one union subscription per view over per-pane areas, not a
     bounding rectangle (`grid/mod.rs:450-489`, `grid/panes.rs:156-172`). Paint and accessibility read that one mirror
     (`grid/mod.rs:502-548`).
   - Slow subscribers and flow control. One flight per subscription; further deltas coalesce into one reset
     (`publication.rs:1-2, 144-158`; test `server.rs:5596-5625`). A publication is posted whole, so there is no
     transport window to deadlock (`publication.rs:283-288`). Acks bypass the request queue (`port.rs:179-206`).
   - Coverage, retries, accessibility. `Geometry.coverage` exists (`projection.rs:1300-1302`) and the semantic projection
     uses it to tell pending from empty (`render_plan.rs:1257-1268`). Command retries go through a bounded journal that
     never runs an identity twice (`crates/calc-host/src/envelope.rs:36-91`). The semantic projection is a 4,096-cell
     window with merged cells as one cell and refusal reasons (`render_plan.rs:1222-1300`).
   So Stages B and C of the answer are mostly done. Do not schedule them as written.

2. **The answer's "same-revision presentation" risk is the reverse in code: conditional formats cause over-delivery.**
   If any conditional-format rule exists anywhere in the workbook, every committed change or recalculation resets every
   subscription (`crates/calc-host/src/session.rs:1838-1853`). The worker turns a host reset into a full resend
   (`publication.rs:144-146, 196-197`), and the page restages from empty (`tile_mirror.rs:121-134`). A one-cell edit in
   such a workbook re-sends the whole visible interest, every fragment, every time. This is the largest amplification in
   the route, and the answer did not see it. Fix: reset only subscriptions whose interest meets a rule's areas
   (17.a).

3. **The demonstrated costs the answer found are real and larger than it says.**
   - Notebook paging: `outline_keys` starts from every input cell of the sheet (`apps/dna-calc-worker/src/tree.rs:12`),
     then each key is checked against every placement (`server.rs:3896-3899`). K is the sheet's input-cell count, not a
     page. `seen.contains` is also quadratic in occupied areas (`tree.rs:20`).
   - Retained-byte recount: the worker folds all retained cells on every update (`publication.rs:107-110`), including
     the empty geometry-only update every stream gets on each revision (`server.rs:3799-3811`). The page mirror does the
     same per fragment (`tile_mirror.rs:189-193`), so a 256-fragment publication folds the staging root 256 times.
   - Not in the answer: a non-reset admission clones the whole committed root into staging (`tile_mirror.rs:125-134`).
     So each root exists four times: host `subscription.cells` (`session.rs:1829, 1873`), worker `TileStream.cells`
     (`publication.rs:11`), page committed mirror, page staging copy.
   - Not in the answer: on the desktop, tile bytes cross WebView2 as base64 inside a string and are decoded in JS
     (`apps/dna-calc/web/desktop-bridge.js:27-40, 50-57`).

4. **Reply causality is a real gap, but narrower than the answer states.** The page applies each event synchronously on
   arrival (`port.rs:392-431`, `controller.rs:930-945`), so "a reply finds its subscriptions' cells already applied"
   (`worker.rs:104-105`) holds while no flight is outstanding. When one is, the change is held as pending
   (`publication.rs:145, 192`) and the reply is posted first; the slow-subscriber test shows an entry producing no
   events (`server.rs:5603-5605`). Two more cases: the desktop posts add-in replies before events (`apps/dna-calc-desktop/src/app.rs:344-349`), and
   the formula bar reads the selection mirror (`controller.rs:719-722`) while the grid reads its own
   (`grid/mod.rs:502`). Those two mirrors complete in separate messages. Fix: state the real guarantee and audit the
   reply callbacks that read mirrors (17.b).

5. **Linked boxes: the answer designs for an idea the owner has since redirected, and misses the existing canvas
   skin.** At master (41e3181b), boxes are "an alternate display and formula editor over the sheet", drawn from
   precedents and dependents, with the sheet as source of truth. Only what Excel exposes may drive the view
   (`LINKED_BOXES_SKIN.md:50-62` at master). A canvas skin already draws the active cell's precedents and dependents
   as wired chips (`apps/dna-calc/src/canvas.rs:1-12`, registered at `apps/dna-calc/src/app.rs:40`). The layout model already has a
   Canvas view of placements at world coordinates, kept apart from block identity (`layout.rs:3-11, 136-146`), saved
   in the DNA view manifest as a custom XML part (`crates/calc-doc/src/load/manifest.rs:4`). So the answer's Stage D/F
   (name-backed notebook blocks, a new box skin) is not the next step. Its presenter binding matrix (3.7) and
   experiments E3, E7 and E8 remain useful inputs for dc-lvlj.

6. **E8 fills a recorded gap and is cheap.** The cdoc-m2.1 tests say that Excel opening and re-saving a package with
   the manifest and hidden locator names "is the owner's Excel step, which these cases do not show"
   (`crates/calc-doc/tests/acceptance.rs:8666-8668`). No open bead covers it. Fixtures exist
   (`crates/calc-doc/tests/fixtures/cdoc_m2_1/`). See 17.c.

7. **Owner-rule tensions.** "Begin WebGPU only if renderer measurements identify a bottleneck" (Stage F, 6.9) cuts
   against the owner's direction of a WebGPU-friendly grid that is fast from the start. Keeping `RenderPlan` as the
   shared IR is consistent with that direction; the deferral wording is not. The performance budgets (section 4) must
   stay targets, never per-commit checks. Stage A's route tracing and instrumentation is process with no product
   output; this harvest is that trace. The interaction table assigns formula binding to the object-model/JS review,
   but the binder lives in calc-fml.

---

## Disposition of every section

Bead keys: 17.a-17.d are proposed below (section 6). "Cross-ref N" means the item belongs to the harvest of answer N
(14 object model/JS, 15 graph/recalc, 16 engine-host-doc, 18 concurrency) and is not duplicated here.

| Section | Content | Disposition |
|---|---|---|
| 1 intro | Keep the vertical architecture; finish consistency, lifecycle, resources | no action: agrees with the code |
| 1 "What to keep", row 1 | Host deltas, `dispatch_from` | no action: confirmed (claims 1-2) |
| 1 row 2 | Worker, mirror, admission/completion | no action: confirmed; the "end-to-end proof obligation" is met by tests (finding 1) |
| 1 row 3 | Shared commands, adapter, registry, focus tickets | no action: confirmed (`skin.rs:18-38`, `command.rs:1-43`) |
| 1 row 4 | RenderPlan, Canvas2D/DOM selector | no action: confirmed; WebGPU not implemented (agreed) |
| 1 row 5 | Names and notebook | Name-backed blocks: deferred (waits on dc-lvlj). Passive/explicit split: no action, already true |
| 1 row 6 | Browser and desktop execution | Count budget vs wall clock: cross-ref 18 |
| 1 "Three distinctions" | One skin at a time; linked boxes undecided; tile completion is not calculation completion | no action: all three confirmed. Linked boxes now has owner direction (finding 5) |
| 1 "Reported measurements" | Population table | no action: restates the prompt |
| 2 intro | No corruption defect proved | no action: agrees |
| 2 P1 stale lifecycle | Old-workbook admission invalidates a new mirror | no action: already handled (finding 1). Residual: the page routes by id, not by `stamp.workbook` (`controller.rs:913-926`). Safe only while ids never repeat; noted in 17.b as an optional one-line guard |
| 2 P1 composite-view atomicity | Several streams per view tear | no action for the grid (one union stream). The selection/grid mirror pair: **17.b** |
| 2 P1 reply causality | Reply assumes cells applied | **17.b** (finding 4) |
| 2 P1 aggregate bounds | Many large subscriptions | Staging clone and recounts: **17.a**. An aggregate cap: deferred (waits on simultaneous views; the page opens two subscriptions today, `controller.rs:1031`, `grid/mod.rs:485`) |
| 2 P1 name-backed notebook | Block has no name identity | deferred: waits on dc-lvlj. Confirmed at the model (`layout.rs:108-113`), not only the wire |
| 2 P1 single-operation responsiveness | One slow call blocks edits | cross-ref 18 |
| 2 P2 notebook paging | O(P) + O(PK) | **17.a** (worse than stated, finding 3) |
| 2 P2 retained-byte recount | Fold over every retained cell | **17.a** |
| 2 P2 same-revision presentation | CF change without revision | **17.a**: the code over-delivers instead (finding 2) |
| 2 P2 codec allocation | Clone + JSON per cell | deferred: waits on the performance phase (owner: performance after publish); no wire-format change now |
| 2 P1 alias semantics | Prefix stripping captures locals | dc-lvlj (input to the discussion) |
| 2 earlier-review note | Do not repeat "no transaction closure" | no action: agrees |
| 3.1 ownership list | Doc-model, doc, engine, host, worker, model, adapters | no action: matches AGENTS.md and the code |
| 3.1 state table | Semantic vs presentation state | no action: matches `layout.rs:3-11` (layout is document-only metadata) and the model's view state |
| 3.2 identities | ViewLease, PendingUiEffect, CommittedViewRoot, BlockTarget | ViewId/mount generation: deferred (simultaneous views). Generation, edit-session id and focus tickets exist (`controller.rs:987`, `state.rs:57-107`, `skin.rs:37-38`). BlockTarget::Name: deferred (dc-lvlj) |
| 3.2 routing before mirror | Discard old-workbook messages before the mirror | no action: done by id routing (finding 1) |
| 3.3 one committed root per view | Union interest, explicit coverage | no action: already the grid's design (`grid/mod.rs:450-489`, `projection.rs:1300-1302`) |
| 3.3 publication sequence 1-7 | Capture, admit, stage, complete, read, ack | no action: matches `publication.rs:185-288`, `tile_mirror.rs:69-227`, `controller.rs:945` |
| 3.3 editor draft overlay | Atomic apply must not overwrite a draft | no action: draft lives in `EditSession`, not in the mirror (`state.rs:57-107`). Not tested here |
| 3.3 multi-stream manifest | Barrier manifest across streams | deferred: no view uses two streams for paint. The selection/grid pair goes to 17.b |
| 3.3 incremental facts table | Ten rows | Spill removals, geometry with cells, dictionary lifecycle, interest movement: already true (`publication.rs:34-75`, `tiles.rs` model 51-52, `tile_mirror.rs:109-120`, `session.rs:94`). Styles/CF: **17.a**. Name definition push: deferred (names are paged on request, `names.rs:16-21`). Calculation freshness: no action (`tiles.rs` model 1-5) |
| 3.3 do not parse display text | `CellShown.text` is presentation | no action: agrees |
| 3.4 four receipts | Admission, commit, terminal, applied | no action: already four (`envelope.rs:63-91`; `controller.rs:790-832`; `TileAck`) |
| 3.4 reply contracts | At-least-command vs exact read | **17.b** |
| 3.4 never wait on acks under a lock | Host must not block on views | no action today (acks are fire-and-forget). Related stall: deferred calculation events stop all calculation and commands when any stream stays pending (`server.rs:345, 587-590, 3825-3830`): deferred to simultaneous views |
| 3.4 ingress-specific behaviour | Do not unify UI/VBA/COM/JS timing | cross-ref 14 |
| 3.4 retries | Command identity and outcome record | no action: already true (`envelope.rs:36-91`) |
| 3.5 capacity estimates | 64 MiB, 56 MiB, 2.5 GiB, 192/256 MiB, 258/514 events | no action: arithmetic confirmed (claims 20-23). The four-copy case is real (finding 3) |
| 3.5 zero-copy | Do not call it zero-copy | no action: agrees; desktop base64 adds a copy (finding 3) |
| 3.5 small changes 1-2 | Incremental counters, indexed placement paging | **17.a** |
| 3.5 small changes 3-5 | Shared chunks, no clone to serialize, style atoms | deferred: performance phase |
| 3.5 flow control | Credit vs ack deadlock | no action: no transport window exists (finding 1) |
| 3.5 byte charging everywhere | Charge queued and transferred bytes | deferred: performance phase |
| 3.5 slow subscriber | Acked root + one flight + reset marker | no action: already true (finding 1) |
| 3.6 render plan extension | Pane transforms, shaped runs, font keys, hit regions | deferred: waits on dc-971p.43 (pixel comparison) and dc-ysl.9 (fonts). Note the owner wants the grid WebGPU-friendly now |
| 3.6 GPU caches local | Device loss rebuilds | no action: design note for the later renderer |
| 3.6 geometry bullets | Indexed geometry, merges across panes, overflow, interest lookahead | no action: merges across panes (`render_plan.rs:73-74, 1249-1251`), overflow anchor vs rect (`render_plan.rs:78-80`), overflow lookahead (`server.rs:3567-3610`) exist. Sparse variable dimensions: not checked |
| 3.6 editing bullets | IME, source version, draft conflicts, tickets | no action: tickets and edit-session ids exist; IME not checked here (dc-971p) |
| 3.6 accessibility bullets | Semantic projection from committed facts | no action: already true (`render_plan.rs:1222-1300`). Announcement coalescing not checked |
| 3.6 DOM and Canvas2D same cases | One interaction suite | no action: the painter comment says the same (`grid/painter.rs:1-2`) |
| 3.7 notebook: block vs placement | Separate identities | no action: already true (`layout.rs:3-11, 94-104`) |
| 3.7 notebook: names via NameRecord/EvaluateName | Reuse, no flattening | deferred: dc-lvlj |
| 3.7 notebook: revision-qualified cursors | Pages from one revision | deferred: `Notebook` has page and total but no revision (`projection.rs:1871-1876`); take it with 17.a's paging fix only if trivial |
| 3.7 notebook: runtime name ids vs Excel resave | Reconcile external edits | **17.c** (E8 tests it for locator names) |
| 3.7 linked boxes | DNA Calc only; wiring; arrays; cycles; unwire; custom XML; reverse import | dc-lvlj. Consistent with owner rules (OneCalc values-only; cycles as Excel allows). Superseded in part by the owner direction at 41e3181b (finding 5) |
| 3.7 presenter/interpreter | Binder + source spans; binding table; contexts; completion; diagnostics; rename | dc-lvlj: the binding table is the right test list for any presenter. Excel facts it needs: **17.d** |
| 3.8 multiple views | View map, per-view state, shared history | deferred: no owner request; one skin at a time today (`skin.rs:4-6`) |
| 3.8 web vs desktop owners | One owner per session | no action: true. Each page Worker gets its own server on both platforms (`desktop-bridge.js:21-25`, `apps/dna-calc-desktop/src/app.rs:257-264`) |
| 3.8 macro policy paragraph | Off/Safe/Unguarded, Safe/Full | no action: restates owner rules; cross-ref 13/18 |
| 3.8 interaction table | Owners of shared contracts | no action, with one correction: formula binding belongs to calc-fml, not the object-model review |
| 4 Stage A | Trace and instrument the route | no action: process with no product output; this harvest traced it |
| 4 Stage B | Lifecycle guards, one root, reply barriers | Lifecycle and one root: no action (done). Reply barriers: **17.b** |
| 4 Stage C | Accounting, paging, credits, recovery | Accounting and paging: **17.a**. Credits: no action (no window). Recovery: done |
| 4 Stage D | Name-backed notebook editing | deferred: dc-lvlj |
| 4 Stage E | Simultaneous views | deferred: no owner request |
| 4 Stage F | Linked boxes; defer renderer | Boxes: dc-lvlj. Renderer: deferred, with the WebGPU caveat (finding 7) |
| 4 performance budgets | Latency and memory targets | deferred: performance phase; never as per-commit checks |
| 4 compatibility paragraph | Same populations before/after | no action: AGENTS.md honesty rules already say this |
| 5 common capture | Record builds, modes, bits, events | no action: the capture lineage records this |
| 5 E1 | Passive name inspection vs evaluation | **17.d** (optional part) |
| 5 E2 | Relative names and window context | **17.d** |
| 5 E3 | Dotted names, LET/LAMBDA binding, rename | **17.d** |
| 5 E4 | Scoped calculation, spills, save caches | cross-ref 15/16 |
| 5 E5 | Event order by ingress | cross-ref 14 |
| 5 E6 | Office.js sync and partial failure | cross-ref 14 (needs the Office.js runtime) |
| 5 E7 | Named cycles and name arrays | **17.d** (also feeds dc-hul4 and the Circular Reference workbook) |
| 5 E8 | Custom XML and external name edits | **17.c** |
| 5 E9 | Slow foreign calls and cancellation | cross-ref 13 (dc-6332.13, dc-6332.15) and 18 |
| 5 fault-injection: multi-tile spill | Old or complete root | no action: tested (`tile_mirror.rs:407`, `server.rs:5626-5661`) |
| 5 fault-injection: slow subscriber | Plateau, recovery | no action: tested (`server.rs:5596-5625`) |
| 5 fault-injection: credit window | Deadlock | no action: no window |
| 5 fault-injection: rapid switching | No stale effects | no action: tested for late tile events (`controller.rs:4794-4846`); focus tickets exist |
| 5 fault-injection: reset during staging | One new root | no action: the rule is in code (`tile_mirror.rs:116-119`); a specific test was not checked |
| 5 fault-injection: malformed wire | Bounded rejection | no action: decoder tests for truncation and bad lengths (`dna-calc-protocol/src/tiles.rs:273-363`) |
| 5 fault-injection: malformed authored batch | Validate before mutation | cross-ref 14 |
| 5 fault-injection: linked-name parsing | Round trip | dc-lvlj |
| 5 fault-injection: coalesced edit then undo | Both outcomes kept | **17.b** (add as one case if cheap) |
| 6.1 | Stop reopening whether the model/tiles should exist | no action: agrees |
| 6.2 | Delete the recount and pre-page materialization | **17.a** |
| 6.3 | "Events before reply" is not the full contract | **17.b** |
| 6.4 | Remove redundant skin command paths once proven | no action: dc-971p.27 (closed) did U1; nothing named |
| 6.5 | No notebook/box evaluator, graph or hidden sheet | no action: agrees with owner rules |
| 6.6 | No prefix substitution | dc-lvlj (the idea note already says syntax tree, not text) |
| 6.7 | Not every dependency edge is a wire | dc-lvlj |
| 6.8 | Measure before replacing the binary format | deferred: performance phase |
| 6.9 | Defer WebGPU until a bottleneck | deferred, with the owner caveat (finding 7) |
| 6.10 | Keep preservation refusals | no action: agrees (dc-mndh) |
| 6.11 | Capability policy is not isolation | no action: restates the owner rule |
| 6.12 | Preserve compatibility evidence | no action: agrees |

---

## Claims checked against the code

| # | Claim (answer section) | Verdict | Evidence at 3b5aa86b |
|---|---|---|---|
| 1 | `HostDelta` has subscription, seq, authored revision, reset, explicit empties (1) | confirmed | `session.rs:112-126` |
| 2 | `dispatch_from` is one ingress for UI and automation (1) | confirmed | `session.rs:887-900` |
| 3 | Admission, fragments, completion; mirror checks continuity and stages (1) | confirmed | `publication.rs:283-288`, `tile_mirror.rs:69-227` |
| 4 | Full fragment decoding, completion and consumer installation not shown (1) | not checkable by the answer; true in code | `dna-calc-protocol/src/tiles.rs:121`, `port.rs:392-431`, `controller.rs:930-945` |
| 5 | Exactly one skin is shown; the old one is dropped (1) | confirmed | `skin.rs:4-10, 74-76` |
| 6 | Tile completion is not calculation completion (1) | confirmed | `dna-calc-model/src/tiles.rs:1-5, 25-28` |
| 7 | Old-workbook admission into a new mirror causes Identity then invalidate (2) | partly | True of `ViewMirror` alone (`tile_mirror.rs:71-80, 93-98`). Unreachable in the page: ids never repeat (`controller.rs:4064-4069`), mirrors cleared (`:990`), server drops queued events (`server.rs:1691-1693`) |
| 8 | `ProjectionFailed` carries only id and reason (2) | confirmed | `projection.rs:1785-1788`; harmless for the same reason as 7 |
| 9 | Streams get distinct transaction ids per host change (2) | confirmed | `server.rs:3812-3817` |
| 10 | So one scene over several streams can tear (2) | partly | The grid uses one stream per view (`grid/mod.rs:479-489`). The formula bar reads the selection stream (`controller.rs:719-722`), a second root |
| 11 | Reply assumes subscribed facts are applied because events were posted first (2) | partly | True when no flight is outstanding (`port.rs:392-431`). False while one is: change held pending (`publication.rs:145, 192`); test `server.rs:5603-5605`. Desktop add-in replies precede events (`apps/dna-calc-desktop/src/app.rs:344-349`) |
| 12 | Local limits exist; no global cap or sharing shown (2) | confirmed | `tiles.rs` model 9-19; cap of 32 streams enforced (`server.rs:1135`); no aggregate byte cap found |
| 13 | Notebook block has no name identity (2) | confirmed, and deeper | `projection.rs:1878-1883`; the doc model has only Prose and Calculation blocks over cells (`layout.rs:108-113`) |
| 14 | A count budget of 64 evaluations, not wall clock (2) | confirmed | `worker.rs:97`; desktop extra servers use 6 ms wall-clock slices (`apps/dna-calc-desktop/src/app.rs:289-291`) |
| 15 | Notebook paging collects all placements before skip/take; O(PK) containment (2) | confirmed, larger | K = every input cell of the sheet (`tree.rs:12`), filtered against P (`server.rs:3896-3899`) |
| 16 | A small edit, even outside a subscription, folds every retained cell (2) | confirmed | `publication.rs:107-110`; every stream is updated on each revision (`server.rs:3799-3811`) |
| 17 | A same-revision CF change may not propagate (2) | wrong (opposite) | Any CF rule makes every change reset every subscription (`session.rs:1838-1853`) |
| 18 | Encoder clones each cell, JSON-encodes, clones the dictionary key (2) | confirmed | `dna-calc-protocol/src/tiles.rs:44-53` |
| 19 | Omitted cells inside the interest are empty; outside is not (3.3) | confirmed | `session.rs:121-124`; coverage on the wire (`projection.rs:1300-1302`, `server.rs:3700-3712`) |
| 20 | 65,536 cells at 1,024 bytes = 64 MiB; 896 bytes = 56 MiB (3.5) | confirmed | Arithmetic; `tiles.rs` model 10, 15-16 |
| 21 | 32 x 80 MiB = 2.5 GiB (3.5) | confirmed as arithmetic | `tiles.rs` model 9, 14. In use: two streams (grid view and selection) |
| 22 | Three or four full copies, 192/256 MiB (3.5) | confirmed | Host (`session.rs:1829, 1873`), worker (`publication.rs:11`), page committed and staging clone (`tile_mirror.rs:125-134`) |
| 23 | 256 records per fragment: 258 and 514 events (3.5) | confirmed | `tiles.rs` model 12, 18; record limit binds before the 512 KiB byte limit for ordinary cells |
| 24 | Transferred buffer is copied into a `Uint8Array` first (3.5) | confirmed | `worker.rs:40-44, 117-121`; desktop adds base64 (`desktop-bridge.js:27-40`) |
| 25 | `PlannedCell` has spans, borders, clip, measured text, anchor and rect (3.6) | confirmed | `render_plan.rs:59-83` |
| 26 | Canvas2D painter with a DOM/Canvas selector (1, 3.6) | confirmed | `grid/painter.rs:1-35` |
| 27 | Passive names and anchor-qualified `EvaluateName` (1, 3.7) | confirmed | `names.rs:1-34`, `projection.rs:828-832` |
| 28 | `EvaluatePart` uses UTF-16 offsets (3.7) | confirmed | `projection.rs:833-840` |
| 29 | Notebook projects saved placements plus worksheet cells (1) | confirmed | `server.rs:3874-3900` |
| 30 | The automation comment covers one compatibility observation (3.4) | confirmed | `session.rs:892-894` |
| 31 | Retries need command identity and a bounded record (3.4) | already true | `envelope.rs:36-91` (ReceiptExpired, IdentityReused) |
| 32 | Slow subscriber needs one flight and a reset marker (3.5) | already true | `publication.rs:144-158`; test `server.rs:5596-5625` |
| 33 | Union interest must not become a bounding rectangle (3.3) | already true | `grid/panes.rs:156-172` (one area per pane) |
| 34 | Interest planning must include overflow sources (3.6) | already true | `server.rs:3567-3610` (2,048-column overhang, 32,768-cell limit at `:4522-4523`) |
| 35 | Semantic accessibility projection needed (3.6) | already true | `render_plan.rs:1222-1300` |
| 36 | Layout belongs in the existing custom-part mechanism (3.7) | confirmed | `crates/calc-doc/src/load/manifest.rs:4`, `custom_xml.rs:4` |
| 37 | "Nothing new is stored" should be narrowed (3.7) | confirmed | The idea note itself says layout lives in custom XML (`LINKED_BOXES_SKIN.md:17-18`) |
| 38 | Formula binding is owned by the object-model/JS review (3.8 table) | wrong | The binder is calc-fml, outside all five reviews except its interface |
| 39 | Tracker reports refusals for preserved chart/dialog references (6.10) | confirmed | `.beads/issues.jsonl:455` (dc-mndh) |
| 40 | Excel keeps or drops custom XML on save; renames update names (E8) | not checkable here | No recorded run: `acceptance.rs:8666-8668`. Goes to 17.c |
| 41 | Excel LET/LAMBDA locals shadow workbook names (E3 H1) | not checkable here | Goes to 17.d |
| 42 | Excel Name Manager may evaluate names (E1 H3) | not checkable here | Goes to 17.d |

---

## Experiments

"Service" means the Excel answer service (formula values, batched). "Desktop" means the Windows Excel desktop through COM
(events, UI, save/reopen, windows, names editing). Predictions are hypotheses.

| Exp | Question | Inputs | Outcome per hypothesis | Calls | Where | Bead |
|---|---|---|---|---|---|---|
| E8 | Does Excel keep the DNA view manifest and hidden locator names through its own save, and move a locator with a structural edit? | The three cdoc-m2.1 packages (`notebook.xlsx`, `manifest-v1.xlsx`, `manifest-v2.xlsx`). Branches: open and save; edit a value and save; insert a row above a block anchor and save; SaveAs `.xlsm` and `.xlsb` for one package. Then reload each in DNA Calc and read `LayoutWarning` | H1: item, itemProps and rels survive, locator names survive and shift with the row; DNA Calc reloads with no warning and the block at the new anchor. H2: parts renumbered or rewritten but parsed content equal; no warning. H3: part dropped; DNA Calc reports an empty layout. H4: locator does not shift (it would if it is an ordinary name); block points at the old cell | 3 x 3 + 2 = 11 workbook runs | Desktop | 17.c |
| E3 | How do dotted names bind next to LET/LAMBDA locals, unqualified names and sheet-local names? Which dotted spellings are legal? | Names `Box.Input=11`, `Input=19`, Sheet2-local `Box.Input=29`; the five formulas of E3 on both sheets. Legality: `Names.Add` for about 20 spellings (`Box.A1`, `Box.R1C1`, `A1.Box`, `Box..x`, `Box.1`, Unicode, quoted-sheet scope). Rename `Box.Input` via `Name.Name`, and delete/add, then save/reopen | H1: locals win: LET/LAMBDA give 111 and 129. H2: names win: 30 and 48. Rename: H3 dependent formulas re-spelled, values kept; H4 formulas unchanged and broken | 10 cells + 20 legality calls (service); 2 rename branches (desktop) | Service + desktop | 17.d |
| E7 | Do named cycles iterate like cell cycles? Does explicit evaluation of an array name install anything? | `Box.A=Box.B+1`, `Box.B=Box.A/2`, C1 `=Box.A`; iteration off, on (100, 0.001), on (1 iteration); F9, Shift+F9, Ctrl+Alt+F9. `Box.Array=SEQUENCE(2,2)` with `=Box.Array` in a cell vs `Evaluate("Box.Array")` | H1: named cycles iterate to near A=2, B=1 with recorded bits. H2: refused or held. H3: evaluation returns a 2x2 array, no spill. H4: extra effects | 9 runs (desktop, iteration settings and keys) + 2 cells (service) | Desktop + service | 17.d |
| E2 | Does an explicit evaluation of a relative name use the caller or the active cell? | `Rel` = `=Sheet1!RC[-1]` defined at B2; A2=11, A3=23; B2 and B3 `=Rel`; two windows with B2 and B3 active; `Application.Evaluate` vs `Worksheet.Evaluate`; `RefersTo`/`RefersToR1C1` before and after editing the name from a third cell | H1: caller decides for cells (11/23). H3: `Application.Evaluate` follows the active window's active cell. H4: re-entering the definition from another cell rebases it | 6 runs | Desktop | 17.d |
| E1 | Does name inspection or the Name Manager call UDFs? | VBA `Probe` logging to a module array; `Box.Result=Probe("name",Box.Input)`; four branches from a saved baseline; arithmetic control | H1: listing is passive. H3: Name Manager evaluates (calls logged). H4/H5: explicit evaluation detached vs installing | 4 branches x 2 controls = 8 runs | Desktop (UI) | 17.d, optional |
| E4 | Scoped calculation, spills, save caches | As in the answer | As in the answer | about 12 runs | Desktop | cross-ref 15/16 |
| E5 | Event order by ingress | As in the answer | As in the answer | 3 ingress x 2 modes = 6 runs | Desktop (Office.js part needs its runtime) | cross-ref 14 |
| E6 | Office.js sync and partial failure | As in the answer | As in the answer | 4 runs | Office.js runtime | cross-ref 14 |
| E9 | Slow XLL call and cancellation | As in the answer | As in the answer | about 4 runs | Desktop (XLL) | cross-ref 13/18 |

DNA Calc product checks (no Excel; tests through the route the application runs):

| Check | Question | Input | Expected after the fix | Bead |
|---|---|---|---|---|
| P1 | Does a reply arrive before its change's tiles when a flight is outstanding? | In-process transport; subscribe; withhold the ack; `EnterCell`; read the mirror in the reply callback | Today: old value (by code reading). After 17.b: the documented guarantee holds, and each callback that reads a mirror waits or reads reactively | 17.b |
| P2 | Cells published per one-cell edit with one CF rule outside the interest | 1,536-cell dense viewport; a CF rule on Z1000; edit A1 | Today: about 1,536 cells per edit (by code reading). After 17.a: 1 cell | 17.a |
| P3 | Cells cloned when staging a one-cell delta | Same mirror, CF removed | Today: the whole root. After 17.a: the delta only | 17.a |
| P4 | Keys visited to serve notebook page 1 | A sheet with 100,000 input cells and 1,000 placements | Today: 100,000 keys, up to 10^8 containment checks. After 17.a: bounded by the page | 17.a |

---

## Proposed beads

**Start first: 17.c, 17.a, 17.b.** 17.c is cheap, fills a recorded gap, and every layout skin (notebook, tree, canvas,
boxes) depends on the manifest surviving Excel. 17.a removes the biggest amplification in the route with small,
local changes. 17.b fixes a stated guarantee that is false under load.

**17.a — Projection amplification: CF resets, staging clone, recounts, notebook paging.** Priority 2. Sonnet.
No dependencies. Read AGENTS.md first.
- What: four local fixes in one batch, one check after.
  1. `crates/calc-host/src/session.rs:1838-1853`: when conditional rules exist, reset only the subscriptions whose
     interest meets a rule's areas (the rule areas are already gathered in `snapshot`, `:1708-1712`). Other
     subscriptions take the ordinary diff path. When the rules themselves changed (`refresh_rules`), use the areas
     of the rules before and after the change, so removing a rule still repaints its old cells.
  2. `apps/dna-calc-model/src/tile_mirror.rs:125-134`: stage a non-reset publication as a delta (changed cells and
     removals) applied to the committed root at completion, instead of cloning the root at admission. Keep "no fragment
     is observable until completion".
  3. Keep a running retained-byte count in `TileStream` (`apps/dna-calc-worker/src/publication.rs:107-110`) and in
     `Staging` (`tile_mirror.rs:189-193`), updated on insert and remove.
  4. Notebook paging (`apps/dna-calc-worker/src/server.rs:3862-3928`, `tree.rs:6-46`): find the requested page without
     building the set of every input cell, and test containment against an index of placed anchors. Keep the order and
     the reveal behaviour.
- Acceptance: existing tile, mirror and notebook tests pass unchanged. New tests through the server route: a one-cell
  edit with a CF rule outside the interest publishes one cell (P2); a one-cell delta does not copy the root (P3); page 1
  of a 100,000-input sheet visits a bounded number of keys (P4). CF appearance tests still pass for rules inside the
  interest, including removal of the last rule.

**17.b — The reply-visibility guarantee and the two mirrors a view reads.** Priority 2. Opus for the short design
decision, then Sonnet. Depends on nothing. Read AGENTS.md first.
- What: `apps/dna-calc-worker/src/worker.rs:104-105` says a reply finds its subscriptions' cells applied. That is false
  while a flight is outstanding (`publication.rs:145, 192`). Decide one of: (a) state the weaker guarantee and make
  every reply callback that reads a mirror read reactively or wait for the next `TileAck`; or (b) carry
  `visible_through` (per-subscription `through`) on the reply and let the page delay those callbacks. Prefer (a) unless
  a call site needs (b). Also: post desktop add-in replies after events (`apps/dna-calc-desktop/src/app.rs:344-349`),
  as the other paths do. Make the formula bar and grid show the same committed change: either the formula bar reads
  the active cell from the grid's mirror when it covers the cell, or the selection and grid publications of one
  host change complete in one message. Optional: compare `stamp.workbook` with the current workbook before routing a
  tile event (`controller.rs:913-926`).
- Acceptance: a list in the commit message of every reply callback that reads `selection` or a view mirror, each
  resolved. One in-process test (P1) with an outstanding flight. One test that an edit followed by undo, both before
  the ack, ends with the undone value visible and both receipts delivered.

**17.c — Excel round trip of the DNA view manifest and locator names (E8).** Priority 2. Sonnet with the Windows
Excel desktop. No dependencies. Read AGENTS.md first.
- What: the cdoc-m2.1 cases test our own save and reload only; Excel's save was never checked
  (`crates/calc-doc/tests/acceptance.rs:8666-8668`). Run the 11 workbook runs of E8 on the fixtures in
  `crates/calc-doc/tests/fixtures/cdoc_m2_1/` through the existing Excel session tooling. Record per run: the package's
  custom XML parts and rels (names and hashes), the hidden locator names and their `RefersTo`, and DNA Calc's
  `LayoutWarning` and block anchors on reload.
- Acceptance: a committed record of the 11 runs in the capture lineage. For each outcome that is not H1/H2, one bead
  naming the calc-doc change (for example: reconcile a moved anchor, or report lost layout) rather than a silent
  attach.

**17.d — Excel facts about names that a presenter and the boxes view need (E3, E7, E2, optional E1).** Priority 3.
Sonnet. Depends on nothing; feeds dc-lvlj. Read AGENTS.md first.
- What: record the name-binding, legality, rename, named-cycle, array-name and relative-name facts of section 5,
  through the answer service where values suffice and the desktop where windows, keys, iteration settings or renames
  are needed. Put the value questions in a new research workbook written by the existing generator (AGENTS.md: never
  add to a captured workbook).
- Acceptance: the captures committed; a short note under `docs/architecture/ideas/` beside `LINKED_BOXES_SKIN.md`
  listing which presenter rules from the answer's binding table (3.7) the facts confirm or refute. Where DNA Calc
  differs from a recorded Excel fact, one root-cause bead.

---

## Deferred list

| Item | Answer section | Waits on |
|---|---|---|
| Name-backed notebook blocks (`BlockKind`/anchor for a name), `BlockTarget::Name`, name pages in the notebook | 2 P1, 3.2, 3.7, Stage D | dc-lvlj discussion; the owner direction at 41e3181b puts the sheet first |
| Linked-boxes skin itself, wiring gestures, unwire semantics, reverse import | 3.7, Stage F, 6.6-6.7 | dc-lvlj; it should extend `CanvasSkin` and `ViewKind::Canvas` rather than add a skin |
| Formula-name presenter/interpreter | 3.7 | dc-lvlj, then 17.d's facts |
| Simultaneous views: `ViewId`, per-view selection/focus/editor, view map, aggregate projection cap | 3.2, 3.8, Stage E | An owner request for simultaneous views; also must decouple the global stall where pending tiles stop all calculation and commands (`server.rs:345, 587-590, 3825-3830`) |
| Multi-stream barrier manifest | 3.3 | A view that paints from two streams (none today) |
| Name-definition push to views | 3.3 table | Simultaneous views or a skin that keeps name pages live |
| Render plan extension: shaped runs, font keys, hit regions; WebGPU painter | 3.6, Stage F, 6.9 | dc-971p.43 (pixel comparison) and dc-ysl.9 (fonts). The plan boundary stays WebGPU-friendly now, per the owner |
| Codec: avoid per-cell clone, shared chunks, style atoms, byte charging of queues, desktop base64 | 2 P2, 3.5, 6.8 | The performance phase (owner: performance after publish) |
| Performance budgets (edit-to-ack, scroll-to-paint, memory) | 4 | The performance phase; as targets, never per-commit checks |
| Revision-qualified notebook and name cursors | 3.7 | 17.a's paging change (take it there only if trivial) |
| Count budget vs wall-clock slices; interruption of slow calls | 2 P1, 3.8 | Harvest 18 |
| Ingress-specific ordering, Office.js batches, malformed authored batches | 3.4, E5, E6 | Harvest 14 |
| Scoped calculation and save caches | E4 | Harvests 15 and 16 |
| Slow XLL calls and cancellation | E9 | dc-6332.13 / dc-6332.15 and harvest 18 |
