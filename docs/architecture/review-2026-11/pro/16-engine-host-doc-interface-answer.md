## 1. Verdict on today’s design, with evidence

**Keep the existing architecture and strengthen its contracts. The supplied code already has the right central separation: an immutable source workbook, one current authored `ModelRoot`, derived engine state, an owning host, and a prepared save tied to a particular model/result frontier. The next step is to make that separation hold across every supported product route.**

This is a review of the supplied packet. I have not inspected omitted repository bodies, run the tests, or made fresh Excel measurements. All DnaCalc locations below refer to **`3b5aa86b3ba47cbd216daec70c5d6586ad520deb`**. The supplied CalcVBA pin is **`8115aa78135114c1dea79f26cbc12c8a3b00a4cf`**, but the packet contains no CalcVBA implementation excerpt sufficient to validate its execution or persistence behavior.

### What to keep

**[CODE] The distinction between the loaded workbook and the current workbook is intentional and useful.** `HostState.loaded` is explicitly the initial loaded workbook; `HostState.model` is the current authored root; `HostState.engine` is the derived engine at that revision. `LoadedWorkbook` retains the source package and original model needed for source-preserving saves. Those are different roles, not inherently competing authorities. Keep them.  
Sources: `crates/calc-host/src/state.rs:102–117`; `crates/calc-doc/src/load/mod.rs:415–436`.

**[CODE] Prepared edits already exist.** The host rejects a stale base revision, prepares the document edit, then prepares the engine edit. The shown commit path commits the engine and publishes the authored root before recording `CalcEvent::Committed`. Therefore, “introduce transactions” is too vague and substantially behind the implementation. The useful work is to establish failure neutrality, shared transformation semantics, and observer exclusion for the existing transaction.  
Sources: `crates/calc-host/src/command.rs:338–347`, `:930–942`, `:1021–1042`.

**[CODE] Prepared, pinned saves already exist.** `SavePin` identifies the workbook, authored revision, result view, active calculation, destination, and cache policy. `PreparedSave` owns a `SavePlan`, and writing consumes that plan. Do not repeat the older criticism that serialization necessarily walks a changing live result map. The remaining questions concern how the plan is prepared, what it promises, and how the application commits its output.  
Source: `crates/calc-host/src/save.rs:134–158`.

**[CODE] Reference rewriting already uses the formula implementation.** `ReferenceRewriter` adapts calc-fml’s transformations to the authored model. The documented structural route moves formula nodes and published results, preserves node IDs, and leaves nodes unbound and pending. Keep this conservative implementation until measurements justify selective rebinding.  
Source: `crates/calc-engine/src/structural.rs:4–14`.

**[CODE] The implementation already contains useful refusal boundaries.** Macro-sheet structural changes and unsafe changes involving preserved sheets are refused. Explicit conversion to a non-macro format refuses macro loss. These are working safety boundaries, although a refusal still counts as failure against a requested Excel capability.  
Sources: `crates/calc-doc/src/save/author.rs:387–408`; `crates/calc-doc/src/save/mod.rs:954–963`.

### What the supplied end-to-end evidence establishes

| Product route | What the packet establishes | What it does not establish |
|---|---|---|
| Package → loaded workbook | **[CODE]** Lazy source retention, XLSB conversion through the XML loader, normalized models, preserved-sheet admission, and repair records. | Complete feature fidelity, bounded peak memory for every source, or immutable behavior of every concrete source provider. |
| Command → document/engine commit | **[CODE]** Base-revision validation and preparation of both sides before the shown publication sequence. | That every failed engine preparation is externally neutral, or that all adapters reach this route. |
| Skin edit → undo/redo → save/reopen | **[CODE]** A test body exercises `SkinIntent`, style undo/redo, a refused formatting edit, save, and reopen. | A passing execution at this pin, complete UI coverage, or equivalent JS/VBA behavior. |
| Save request → prepared bytes | **[CODE]** A pinned plan and an explicit cache-selection loop. | Complete CalculateBeforeSave orchestration, destination replacement guarantees, or every cache policy’s Excel parity. |
| Host → visible application | **[CODE]** `HostState::open_report()` exposes repairs. | Delivery of those repairs in each shipped skin, or coherent projection during every concurrent operation. |
| VBA/COM/XLL/JS → workbook behavior | The packet supplies architectural constraints and some relevant host fields. | The executable adapter routes, callback restrictions, thread marshaling, source write-back, or browser refusal behavior. |

Sources: `crates/calc-doc/src/lib.rs:155–173`; `crates/calc-doc/src/load/mod.rs:1881–1919`; `crates/calc-host/tests/round_trip.rs:74–101`; `crates/calc-host/src/state.rs:418–426`; `crates/calc-host/src/save.rs:308–347`.

The round-trip test is particularly valuable because it crosses boundaries. Extend that style of test. Its presence is evidence of an intended check, not a substitute for its execution.

### Reconcile the earlier findings with today’s code

- **[EARLIER-REVIEW] Atomic structural edits:** the October async/address-reuse trace remains a good test. It does not demonstrate a current failure. Today’s preparation/publication machinery must be tested against it.  
  Source: `docs/architecture/review-2026-10/pro/3-architecture-redteam-review.md:159–173`.

- **[CODE] Preserved chart/dialog references:** dc-mndh explicitly refuses structural edits, rename, or removal when preserved sheets make reference rewriting unsafe. The previously dangerous rename case is now guarded in the supplied save path. Coverage of embedded charts on ordinary worksheets remains a separate question.  
  Source: `crates/calc-doc/src/save/author.rs:396–408`.

- **[CODE] Repair reporting:** it now reaches `HostState`. The remaining UI-delivery question must not be reported as “the host discards repairs.”  
  Source: `crates/calc-host/src/state.rs:418–426`.

- **[CODE] XLSB:** the accurate limitation is **no authored XLSB writer**, not “no XLSB save route.” Original binary preservation exists when the computed change set is empty. Cache or view changes can make that condition false even without a cell edit.  
  Sources: `crates/calc-doc/src/save/mod.rs:267–275`, `:984–997`.

- **[CODE] XLSB conversion is not universally lossless.** Its documentation explicitly lists refused features and omitted properties, including a data-bar compatibility downgrade. That is a declared limitation that must remain visible in fidelity claims.  
  Source: `crates/calc-doc/src/xlsb/mod.rs:16–21`.

### The measurement populations remain separate

| Population | Reported result | Permitted conclusion |
|---|---:|---|
| Oracle formula cells | 76,903 / 78,264; 1,361 failures | **[MEASURED-EXCEL]** Reported agreement for that corpus and its recorded cases. |
| Full bank | 3,895,333 / 3,910,795; 15,462 failures | **[MEASURED-EXCEL]** Reported bank result; not a persistence or host-route certification. |
| Capture batch 1 | 130,849 / 134,577; 289 captured files out of 486 selected | **[MEASURED-EXCEL]** Agreement within the captured subset; not a rate for the whole selected batch. |
| Enron saved values | 12,961,186 / 19,666,149 saved-value agreements; **zero live passes** | Agreement with caches of unknown freshness. It supplies useful triage evidence, not live Excel answers. |

Sources: `docs/research/gamma-family-2026-11-12.md:305–311`, `:344–348`; `docs/plan/capture-batch-1-results.md:24–39`; `docs/plan/enron-saved-cache-triage.md:8–20`, `:35–56`.

**Verdict:** the architecture deserves consolidation. Its largest unresolved risk is disagreement among an accepted edit, the engine state derived from it, retained references, and the package eventually emitted—not the absence of an immutable model or save pin.

---

## 2. Specific defects and risks, ranked

The table ranks consequences, not certainty. **The excerpts do not demonstrate a current silent-corruption bug.** They do demonstrate some limitations and repeated work, and expose several high-impact contracts whose implementations are omitted.

| Priority | Exact pinned location | Trigger and observable consequence | Status | Smallest reproducer or missing evidence |
|---|---|---|---|---|
| **P1 — late-result correctness** | `crates/calc-engine/src/structural.rs:7–10`; `crates/calc-host/src/state.rs:123–127` | Structural edits preserve node IDs and move results. An old async completion could be admitted to the wrong formula lifetime if identity checks are insufficient. | **[INFERENCE]** Admission code is absent. Existing authority fields are encouraging. | Start slow UDF, insert a row, reuse the old address, deliver completion; repeat through undo and table rebuild. Inspect accepted token, caller, value, and invocation count. |
| **P1 — transaction failure neutrality** | `crates/calc-host/src/command.rs:941–942`, `:1021–1024` | Engine preparation receives mutable engine state; table changes rebuild after the shown commit. A recoverable failure or improperly exposed intermediate state could leave model and engine inconsistent. | **[INFERENCE]** No failed commit is shown; do not invent one. | Inject a recoverable prepare failure after meaningful engine work. Verify unchanged published root, result view, pending work, history, and notifications. Inspect rebuild lifetime handling. |
| **P1 — stale array/spill caches** | `crates/calc-host/src/save.rs:321–345` | `PreservePermittedCaches` visibly excludes arrays and current spills; dirty formulas then skip the later path. Save may lose permissible stale caches and cause extra evaluation on reopen. | **[CODE]** Branch limitation. **[INFERENCE]** Final cache loss depends on omitted serialization behavior. | Manual, CalculateBeforeSave=false; dirty a legacy array and a dynamic spill with established caches. Inspect emitted caches/flags and reopen callback counts. |
| **P1 — normalized model versus original bytes** | `crates/calc-doc/src/load/mod.rs:1883–1917`; `crates/calc-doc/src/save/mod.rs:89–95` | Repairs normalize the loaded model, but save’s baseline model is already normalized. Model equality alone cannot reveal that source bytes still contain a repair-causing defect. | **[INFERENCE]** The repair-to-save implementation is omitted. | Open duplicate IDs, dangling relationship, and array-inner-formula fixtures; ordinary Save without edits; inspect source records and Excel repair prompts on reopen. |
| **P1 — edit/save capability disagreement** | `crates/calc-host/src/command.rs:920–937`; `crates/calc-doc/src/save/author.rs:387–408` | An edit may be accepted through one route and later refused by persistence because it affects opaque references. Users could discover too late that their revision cannot be saved to the intended format. | **[CODE]** Both host and save refusals exist. **[INFERENCE]** Complete route agreement is unproved. | Same rename, axis edit, and cut/move through skin, JS, and VBA; compare immediate receipt, saveability status, undo, and save result. |
| **P1 — source stability** | `crates/calc-doc/src/lib.rs:155–167`; `crates/calc-doc/src/source.rs:62–74` | The source is read lazily and during save. External in-place changes could mix bytes from different source versions unless providers freeze or validate them. | **[INFERENCE]** `Send + Sync` is not an immutability guarantee; concrete providers may already enforce one. | Modify backing bytes after open and before a deferred read/save. Require pinned original content or a typed source-changed failure. |
| **P1 — failed physical save** | `crates/calc-doc/src/source.rs:77–80`; `crates/calc-host/src/save.rs:157–158` | A sink can fail after partial output. An immutable save plan does not itself protect an existing destination from truncation. | **[CODE]** The shown sink is append-only. **[INFERENCE]** The application may already provide replacement semantics. | Fail writes at several offsets and final replacement. Inspect original destination, temporary output, saved frontier, and visible error. |
| **P2 — inconsistent transform context** | `crates/calc-engine/src/structural.rs:57–63`; `crates/calc-host/src/command.rs:1029–1034` | Formula sheet-removal transforms receive old sheet order; `host.references` receives an empty order. A supported reference form needing that context could diverge. | **[INFERENCE]** Registry behavior and supported handle forms are omitted. | Exercise any supported retained reference requiring sheet-order semantics. If 3-D handles are unsupported, verify explicit refusal; do not fabricate such an API. |
| **P2 — identity misuse** | `crates/calc-doc-model/src/root.rs:248–268`; `crates/calc-host/src/state.rs:109–112` | Structural equality excludes lineage. Using equality or revision alone to authorize a prepared action could confuse two open copies. | **[INFERENCE]** `PartialEq` itself is not defective. | Open identical copies at equal revisions; cross-submit prepared commands, continuations, and result/save pins. Existing `command_scope` should reject the wrong instance. |
| **P2 — repeated structural preparation** | `crates/calc-doc/src/edit.rs:262–280` | Each axis operation is applied to temporary roots, then the full batch is applied again. | **[CODE] Demonstrated repeated work request.** Its cost is unmeasured. | Count rewrite calls, allocations, and copied bytes for batches of 1/10/100 axis operations. Preserve one public batch revision when removing the duplication. |
| **P2 — broad recalculation costs and observables** | `crates/calc-engine/src/structural.rs:7–11`; `crates/calc-host/src/command.rs:1023–1024` | Structural edits leave all nodes unbound/pending; table changes can rebuild. This may be expensive and may affect observable UDF calls if scheduling admits unnecessary invocations. | **[CODE]** Conservative work is documented. Excel divergence is **[INFERENCE]**. | Large sparse workbook plus counted effectful UDFs in affected and unaffected regions; compare work counts and actual Excel callback traces. |
| **P2 — XLSB fidelity and routing** | `crates/calc-doc/src/xlsb/mod.rs:16–21`; `crates/calc-doc/src/save/mod.rs:984–997` | Unsupported conversion features, documented property loss, or cache/view rewrites prevent the requested fidelity or same-format save. | **[CODE]** Declared limitations. App routing is unproved. | Untouched, calculation-only, view-only, scalar-edit, and explicit-conversion branches. Check format, retained features, VBA, and reopen behavior. |
| **P2 — VBA source/signature claims** | `crates/calc-doc/src/xlsb/mod.rs:8–10`; `crates/calc-doc/src/save/mod.rs:954–963` | Preserving a project part can be mistaken for supported source write-back or preserved signature validity. | **Missing evidence**, not a demonstrated missing implementation. | Signed and unsigned projects; cell-only edit, source edit, conversion, reopen, source comparison, signature validation. |

Two qualifications matter:

1. **Internal ID reservations need not roll back.** Sheet identity allocation happens before preparation completes. Consuming an unpublished ID can be correct; the required invariant is no visible object, model, result, history, or event change.  
   **[CODE]** `crates/calc-host/src/command.rs:927–929`.

2. **The closed dc-mndh work must remain closed unless new evidence contradicts it.** The tracker reports admission budgets, streaming corpus processing, defined-name scans, and repair reporting fixes. The packet does not justify reopening all of those as absent.  
   **[DESIGN]** `.beads/issues.jsonl:455`.

---

## 3. Recommended target design and the smallest migration from today’s code

Everything recommended in this section is **[PROPOSED]**, except where explicitly labeled otherwise.

### 3.1 State ownership

Retain the existing owners. Make their authority explicit.

| State | Authoritative owner | Required rule |
|---|---|---|
| Original package, original binary package, ZIP entries, relationships, original source records | `calc-doc::LoadedWorkbook` / `Package` | Immutable source evidence and preservation material. Never consulted as the current editable workbook. |
| Current scalar inputs and formula definitions | `calc-doc-model::ModelRoot` | One current authored root. A formula’s displayed result is not an authored cell input. |
| Formula source, semantic dialect, formula kind, shared membership, legacy-array definition | `ModelRoot`, using existing formula/definition structures | Preserve distinctions needed for evaluation and serialization. Compiled plans remain derived. |
| Defined names, scope identity, table/column identity, sheet order and visibility | `ModelRoot::definitions` and sheet structures | Bind by stable authored identity and explicit scope; serialize file indexes from that identity. |
| Current styles, geometry, modeled rules and objects | Existing root layout/definition subtrees | Source style tables are baselines. Getters, undo, projection, and save use current authored state. |
| Authorable custom XML and any supported VBA source edits | Immutable authored payloads referenced by the root | A save snapshot must not be the first place an authored edit exists. Export-only transformations must be identified as such. |
| Graph, bindings, dirty state, result views, spill outputs, activation lifetimes | `calc-engine` | Derived from qualified authored and input identities. File caches retain their provenance. |
| Command order, callbacks, grants, undo/redo, live handles, selection, active calculation domain | `calc-host` | One owning thread controls publication and external invocations. |
| Source-relative transformation provenance | Existing committed edit lineage, consumed by `calc-doc` | Generated by accepted operations; cannot be reconstructed reliably from final cell differences alone. |
| A save’s source, model, result view, cache policy, settings, and window views | Existing `SavePin` / `DocumentSnapshot` / `PreparedSave` | Immutable once selected. Later host changes cannot alter the image being written. |
| Viewport/display data | Host projection and skin IR | Derived, revision-qualified data. No independent workbook semantics in the renderer. |

**[CODE]** The current snapshot already distinguishes `base`, `model`, values, spill areas, structural edits, cache state, pivot outputs, and custom XML. That is a useful separation of inputs. The unanswered question is where every supplied overlay originates.  
Source: `crates/calc-doc/src/save/mod.rs:82–105`.

In particular:

- Pivot output materialization must identify its calculation or refresh frontier.
- Custom XML changes must identify their authored revision.
- `SaveViews` is a session snapshot, not a second layout model.
- The original `LoadedWorkbook.styles` is legitimate baseline data. The packet does not expose enough of `SkinLayout` or `HostState::styles()` to establish current style duplication.

### 3.2 Identities and invariants

Use the existing identity machinery; do not introduce permanent identities for every empty cell or row.

The important distinctions are:

- **Workbook instance:** two open copies of one file are separate live authorities.
- **Source lineage:** identifies which preserved package and source-to-model mapping a root belongs to.
- **Authored revision:** advances on every committed edit, undo, and redo.
- **Authored entity identity:** sheets, names, tables, and columns survive appropriate renames/moves.
- **Formula occurrence lifetime:** distinct from formula text, template identity, address, and reusable engine slot.
- **Calculation command, input frontier, result view, and destination:** identify which work may publish where.
- **Invocation authority:** an old external completion cannot regain permission because an address or formula text reappears.

Do not change `ModelRoot::PartialEq` merely to make authorization convenient. Structural equality and authority are different operations.

The central invariants should be short enough to enforce directly:

1. Every published engine/model pair names the same authored identity.
2. Every displayed or saved result identifies its provenance and admitted result frontier.
3. Every reference consumer receives the same committed operation and required old context.
4. Every accepted persistence-relevant edit has a known target-format outcome.
5. No callback or observer sees an intermediate publication state.
6. Undo restores authored content into a **new** revision and never rewinds invocation authority.

### 3.3 Extend the prepared edit with one shared change description

The current document and engine preparation paths receive the same `OpBatch`, but independently processing the batch creates room for disagreement. Extend the existing path so the engine consumes the authored result and its change description.

Illustrative types—not a request for a new transaction framework:

```rust
// PROPOSED: model-owned description of one prepared authored change.
struct AuthoredDelta {
    base: ModelIdentity,           // instance/lineage/revision qualification
    next: Arc<ModelRoot>,
    changes: ChangeSet,
    transforms: ReferenceTransformPlan,
}

// PROPOSED: extension of the existing calc-doc prepared edit.
struct PreparedEdit {
    authored: AuthoredDelta,
    persistence: PersistenceImpact,
}
```

`calc-doc-model` owns authored operation semantics. Calc-fml remains the implementation of formula/reference semantics through the existing rewriter interface. Calc-doc checks package preservation obligations. Calc-engine derives its prepared change from `AuthoredDelta`; it does not invent another authored interpretation.

The transformation plan needs the **ordered operations and their context**, including old sheet order. It need not be a general plugin registry.

The same accepted description should drive:

- Formula and defined-name rewrites.
- Graph invalidation and rebinding.
- Tables, rules, modeled chart references, and geometry.
- Shared-formula and array membership.
- Host reference handles and calculation scopes.
- Selection adjustments.
- Source-relative package transformations.
- Undo metadata.

For consumers with different policies, share the operation and context—not necessarily the resulting behavior. A selection and a tracked Range can legitimately react differently.

### 3.4 Operation semantics must remain distinct

| Operation | Required treatment |
|---|---|
| Scalar/formula edit | Update authored input; invalidate affected calculation state; establish a new formula lifetime when appropriate. |
| Name edit | Update the definition and scope; invalidate bindings that depend on resolution, including possible fallback to another scope. |
| Style/geometry edit | Change current authored layout; affect calculation only where an observed host/function dependency requires it. |
| Row/column insertion or deletion | Transform affected coordinates, references, memberships, rules, objects, handles, and source records—or refuse. |
| Sheet insertion/deletion/rename | Maintain stable internal sheet identity; update names and reference text; rebuild file indexes; handle opaque references explicitly. |
| Sheet reorder | Reconsider 3-D span membership even when endpoint text is unchanged. |
| Copy/fill | Create destination occurrences and relocate relative references according to that operation’s measured rules. |
| Cut/move | Move targets and rewrite references that follow them. Do not implement it as copy followed by clear. |
| Table edit | Preserve table/column identity and apply measured expansion/rename rules; invalidate structured-reference binding. |
| Undo/redo | Apply a new authored transition, preserving the applicable reference/handle policy and revoking obsolete work. |

Do not rewrite reference-looking strings indiscriminately. A parsed direct reference and the text passed to `INDIRECT` are different authored constructs.

For relative defined names, preserve an explicit origin/interpretation policy. The earlier synthesis reports A1-based storage and wrapping behavior, while explicitly leaving cell copy/fill behavior unmeasured. That is a reason to keep the operations distinct.  
**[DESIGN]** `docs/architecture/SYNTHESIS.md:19–25`.

Shared formula membership must remain explicit. A bounding rectangle does not by itself establish that every cell is a member. Legacy-array definitions are authored; spill extents and spill values are calculation results tied to an anchor and result frontier.

### 3.5 Commit and failure behavior

The existing route should become:

> Validate target/base → prepare authored delta and persistence impact → prepare engine/reference changes → publish on owner thread → record history and notifications → admit subsequent calculation.

All ordinary recoverable failures belong before publication. If table changes require rebuilding, prepare the replacement derived state before exposing the new root, or otherwise establish that the existing rebuild cannot expose a partially committed host.

Do not invent a “failed engine commit” recovery path unless commit can actually return such a failure.

| Outcome | Visible state |
|---|---|
| Document preparation fails | Existing authored root, published results, history, handles, and notifications remain unchanged. Unpublished ID reservations may remain consumed. |
| Engine preparation fails | Same guarantee; prepared document root is discarded. |
| Commit succeeds | Root, derived engine changes, handle transforms, and history become visible together. |
| Calculation is cancelled | No further publication under revoked authority. Already admitted results and already executed external effects are not magically undone. |
| Save preparation fails | No destination publication; live workbook remains available and unsaved as appropriate. |
| Save writing fails | Existing destination remains intact under the application’s replacement contract; saved frontier does not advance. |
| Save is cancelled after final replacement | Report that the write committed. Cancellation cannot retroactively remove a successful save. |

A callback that has already executed is an observable fact. A transaction must not promise to roll it back.

### 3.6 Undo, source provenance, and history retention

Keep existing shared roots and inverse operations. The history code already supports restoring a popped undo/redo step when restoration did not commit.  
**[CODE]** `crates/calc-host/src/history.rs:443–460`.

However, three histories must not be conflated:

- User undo history.
- The current root’s source-relative transformation provenance.
- External invocation history.

Evicting an old undo step must not erase transformation information still required to save correctly against the original package. Conversely, preserving a source transformation record must not make an external effect replayable.

Undo restoring identical formula text does not revive an old async invocation. Recalculation may produce a newly authorized invocation if the measured command semantics require it; undo machinery must not replay the historical call.

The comment that undo “never restores a deleted scope” describes an implementation policy. Whether a particular COM/VBA/JS handle remains invalid after undo needs an API-specific experiment.  
**[CODE]** `crates/calc-host/src/history.rs:77`.

### 3.7 Save semantics and fidelity

Keep `SavePin` and `PreparedSave`. Extend their documented outcomes and preparation checks.

#### Save sequence

1. Admit the specific command: Save, SaveAs, or SaveCopyAs.
2. Run the appropriate permitted before-save event route.
3. Commit handler edits through ordinary host commands.
4. Capture the model, source, target, settings, relevant host facts, and window views.
5. If calculation is required, admit the appropriate calculation-domain command.
6. Obtain the qualifying result view; wait for required effectful work without duplicating its invocation.
7. Prepare the document image and complete save plan.
8. Write through the platform destination adapter.
9. Publish a receipt identifying the actual saved pin.

The design contract reports P06 distinctions involving BeforeSave, SaveCopyAs, stale caches, and dirty peers. Preserve those recorded profiles and extend them; do not replace them with a universal assumption that save calculation is confined to one workbook.  
**[DESIGN]** `docs/architecture/RECALC_CONTRACT.md:676–692`.

Edits arriving after the pin belong to the live newer root. Completing the old save must not mark that newer root saved accidentally.

Before-save handler edits already committed to the workbook normally remain visible if serialization later fails under this proposed contract. The Excel event/cancellation experiment must establish the exact compatibility profile.

#### Cache policy

| Situation | Required treatment |
|---|---|
| Manual, CalculateBeforeSave=false | Preserve eligible stale caches under the measured policy; retain stale provenance and appropriate file flags. |
| CalculateBeforeSave=true | Require the qualifying calculation outcome. Pending, cancelled, refused, or superseded work produces an explicit save outcome. |
| Automatic mode | Respect the actual automatic calculation state; false does not imply that unrelated automatic work stops. |
| Explicit DropStaleCaches | Keep as an explicit policy. Its additional reopen/UDF calls can be an observable divergence. |
| Formula replaced at the same address | Do not admit an old cache by address alone. Any compatibility-preserving transfer must be explicit and evidenced. |
| Array/spill cache | Treat values, extent, membership, metadata, and provenance coherently; establish stale preservation rules experimentally. |
| Reopen | Import file caches as unverified evidence. Flags do not certify a producing calculation. |

`calcChain`, `fullCalcOnLoad`, `forceFullCalc`, `calcCompleted`, `calcOnSave`, and cell flags are serialized compatibility behavior. They are not substitutes for internal provenance. An untouched source-preservation operation may retain them; an authored save must remove or rewrite invalid hints according to the supported profile.

#### Four independent fidelity promises

| Promise | Evidence required |
|---|---|
| **Whole-ZIP byte identity** | Entire output equals the original archive, including ZIP metadata and layout. An empty logical change set alone does not prove this. |
| **Unchanged-entry preservation** | Compare unchanged part bytes. If raw compressed entry preservation is promised, compare those records separately. |
| **Semantic equality after rewriting** | Compare formulas, types, names, membership, styles, objects, settings, and relevant behavior. |
| **Excel repair-free reopen** | Open the emitted artifact in the stated Excel profile without repair; separately check content. |

The supplied writer excerpts are insufficient to certify all four.

The chart-rename problem illustrates why unchanged bytes can be wrong: a perfectly preserved series formula can still name the old sheet.

#### Can an accepted edit later prove unsavable?

**Yes, for physical failures and some final size/resource constraints. Known semantic impossibilities should be surfaced at edit admission.**

Reuse the writer’s capability checks to return a small persistence assessment:

```rust
// PROPOSED
enum PersistenceImpact {
    SupportedForCurrentTarget,
    RequiresConversion { target: SpreadsheetFormat },
    Unsupported { reason: SaveRefusal },
}
```

This is an assessment, not a second serializer.

Ordinary editing should refuse known unsafe transformations before publication. An explicitly supported mode may retain a temporarily unsavable revision, but the receipt and UI must say so immediately and preserve undo.

Editing an XLSB source can require XML Save As. Do not silently change the requested format, and do not let “no authored edit” stand in for the actual empty-change-set requirement.

### 3.8 Repairs and opaque features

Use two explicit save intentions:

- **Ordinary authored save after a reported repair:** normalize the supported recorded repairs, or refuse if normalization is not implemented.
- **Preserve/export original:** retain original bytes, including defects; do not claim the output was repaired.

Because `Edited.base` is already normalized, retain source-aware repair information sufficient to rewrite the original record. This is provenance, not another workbook model.

Keep silent tolerance separate from repair. The supplied Excel observations distinguish empty `r:id=""` and empty array-member formulas from dangling relationships, duplicate IDs, and text formulas inside array areas. UI opening and automation default-load also differ.  
**[MEASURED-EXCEL]** `crates/calc-doc/tests/fixtures/reader_tolerance/excel-answers.md:30–48`.

Repair reports should reach the application with affected entities and counts. Internal identities must remain stable within the session even when source numeric IDs are repaired.

For macro, chart, dialog, and partless sheets:

- Preserve their package content.
- Preserve modeled names, order, and visibility where supported.
- Do not present them as fully modeled calculation or editing surfaces.
- Transform affected references using supported semantics, or refuse.
- Include embedded charts and other ordinary-sheet objects in coverage analysis; `preserved_kind()` is not evidence of complete package-wide coverage.

### 3.9 VBA, native services, wasm, and the skin

The execution policy has separate dimensions:

| Dimension | Contract |
|---|---|
| Macro level | Off / Safe / Unguarded. |
| Project grant | Whether this project may execute. |
| CalcVBA capability policy | Safe / Full, mapped explicitly from host policy. |
| Invocation context | Worksheet-function restrictions still apply under Unguarded. |
| Native trust | Whether a particular COM/XLL/native service is admitted. |
| Process isolation | An OS/process boundary, if provided; capability checks alone do not supply it. |

Off can preserve VBA bytes without executing them. A runtime source edit is not automatically a persisted `vbaProject.bin` edit. Supported write-back needs an explicit authored source revision, writer contract, and reopen verification. Until that exists, refuse unsupported source persistence honestly.

Distinguish project signatures from package signatures. Retaining signature bytes does not prove their validity after a covered change.

Keep host mutation and VBA callbacks on their owning thread. Pure evaluation can use workers with immutable inputs; workers return proposals/results whose authority the owner checks. A large mutex around the host does not resolve callback reentrancy.

Rust and wasm should use the same semantic core. Native COM/XLL routes remain native-only. Missing facilities need explicit unavailable/refusal behavior, or a measured function-specific error where one is established.

OneCalc continues to expose values-only evaluation and add-in services. Workbook commands, Range handles, persistence, and workbook callback semantics belong to DNA Calc.

The skin receives a qualified projection containing authored revision, result view, status, and visible data. It must show repairs, pending/stale/refused results, save failures, and format limitations without deriving those facts itself. A display-list boundary can accommodate WebGPU-friendly rendering; it does not establish a WebGPU implementation.

### 3.10 Shared contracts across all five reviews

| Shared contract and single owner | Object model / JS | Graph / recalc | Engine–host–doc | Host-to-skin IR | Concurrency / isolation |
|---|---|---|---|---|---|
| **Authored identity and delta — calc-doc-model** | Commands target qualified entities | Consume the committed delta | One edit/save/undo root | Project its revision | Reject wrong-instance publication |
| **Reference semantics — calc-fml, through existing rewriter interface** | Handles apply API-specific policy to the same operation | Rebind/invalidate affected dependencies | Transform authored references and serialization inputs | Adjust projected locations | Decide activation transfer using the same edit context |
| **Result views and activation identity — calc-engine contract** | Reads name an observation frontier | Own dirty/results/activation state | Save selects permitted caches | Display coherent values/status | Validate late results and destinations |
| **Command, event, and grant sequencing — calc-host** | Marshal calls and define batching/reentrancy | Admit calculation commands | Order edits, events, undo, save | Emit post-publication updates | Enforce owner-thread callbacks and grants |
| **Package preservation and save feasibility — calc-doc** | Surface operation/save refusals | Supply results, not package semantics | Own repair/format/opaque-part rules | Present capability and save status | Preserve source and destination guarantees |
| **Projection schema — host-to-skin protocol** | Share host facts where applicable | Supply qualified calculation state | Expose repairs and saved frontier | Own display transport, not workbook meaning | Bound and version messages across workers |

### 3.11 Large-workbook work: measure before replacing storage

The packet identifies concrete costs:

- Structural preparation requests repeated transformations.
- Structural edits invalidate binding broadly.
- Table changes can rebuild the engine.
- Save scans formula cells and constructs maps.
- `ModelRoot::pin()` clones outer collections while sharing deeper state.
- XLSB conversion creates an in-memory XML package while retaining original binary data.
- History retains roots and sometimes loaded source workbooks.

**[CODE]** Sources: `crates/calc-doc/src/edit.rs:262–280`; `crates/calc-engine/src/structural.rs:7–11`; `crates/calc-host/src/save.rs:315–316`; `crates/calc-doc-model/src/root.rs:274–283`; `crates/calc-doc/src/lib.rs:163–167`; `crates/calc-host/src/history.rs:54–72`.

Measure:

- Expanded package bytes, peak resident memory, and simultaneously retained representations.
- Unique bytes retained by history and save/calculation pins.
- Formula rewrite/bind counts and source-string allocation.
- Pages touched and copied per edit.
- Save scan time, compression time, and output buffering.
- Cancellation latency and resource release.

Then remove the demonstrated duplicate preparation work. Do not simply return the final temporary root without checking batch revision semantics.

Keep sparse pages and shared formula sources. Add indexes or more sharing only where measured costs justify them. Avoid eager expansion of full-column references or template regions.

Source-relative save provenance must survive undo eviction. Rebase the source checkpoint only when the new source is durable and old pins/history retain everything they still require.

Reuse the admission and shared concurrency budgeting reported by dc-mndh. Account for conversion buffers, retained roots, and save buffers within that same resource policy.

---

## 4. Staged implementation plan with measurable acceptance per stage

All stages below are **[PROPOSED]**. Preserve existing supported behavior and explicit refusals until a replacement route has evidence.

| Stage | Concrete work and dependency | Assignment | Acceptance, measurement, and stop condition |
|---|---|---|---|
| **S0 — establish route evidence** | Extend existing host round-trip and corpus fixtures with model/result IDs, invocation IDs, source hash, save pin, repairs, and operation outcomes. | **Sonnet:** instrumentation and fixtures. **Opus:** resolve only ambiguous observable contracts. | Execute current checks and retain lineage. Exercise open → edit → calculate → project → save → reopen. Stop when existing behavior is reproducible and missing routes are explicitly identified; do not create another verdict system. |
| **S1 — prove publication invariants** | Audit existing document/engine preparation, table rebuild, stale-base checks, and cross-instance prepared actions. Add failure injection. | **Sonnet:** checks and mechanical changes. **Opus:** commit/reentrancy boundary if current code cannot express it. | Scalar, formula, name, and style edits either commit coherently or publish nothing. Failed preparation leaves live root/results/history unchanged. Wrong-instance continuations fail. Record allocations and commit latency. |
| **S2 — share authored transformations** | Extend prepared edits with the committed delta/context; use it for engine, handles, scopes, and persistence. Resolve the sheet-order discrepancy if relevant. Remove repeated application while preserving revision semantics. Depends on S1. | **Opus:** operation and identity decisions. **Sonnet:** propagation, adapters, fixtures. | Copy, cut, fill, axes, sheet operations, names, tables, arrays, and rules agree through save/reopen. Unsupported opaque transformations retain refusal. Count transform calls; stop when each ordered operation has one semantic execution path. |
| **S3 — authenticate late work and undo** | Validate existing authority/async registries across structural edits, table rebuild, undo/redo, cancellation, and close. Depends on S1–S2. | **Opus:** transfer/restart decisions informed by Excel. **Sonnet:** token propagation and adversarial tests. | No old completion overwrites a new formula. Undo creates a new revision. External calls are not replayed. Callback counts/order match recorded profiles. Measure cancellation latency and retained work. |
| **S4 — complete save guarantees** | Implement or verify repair disposition, array/spill cache policy, same-format eligibility, edit/save capability agreement, source stability, and physical destination replacement. Depends on S1–S3. | **Opus:** unresolved save scope/cache policy. **Sonnet:** writer checks, failure tests, UI status, format fixtures. | Test all four fidelity promises separately. Manual false/true, BeforeSave edits/cancellation, pending async, later edits, sink failure, XLSB preservation/refusal, and explicit conversion have exact outcomes. Stop when every successful save names its actual pin and every refusal preserves live work. |
| **S5 — close adapter and presentation gaps** | Route implemented JS/VBA/skin edits and reads through the same host contracts. Deliver repairs/status. Verify native-only services and macro policy separation. | **Opus:** API batching/reentrancy and grant semantics. **Sonnet:** adapters, protocol fields, product tests. | Native and wasm common routes match. Browser-native service requests fail explicitly. Worksheet restrictions survive Unguarded. VBA callbacks stay on owner thread. No skin-owned semantic cache. Missing features remain declared failures. |
| **S6 — optimize measured bottlenecks** | Optimize rebinding, save traversal, source sharing, buffering, and retention only after attribution. Depends on correctness gates above. | **Sonnet:** local optimizations with evidence. **Opus:** only changes to semantic scheduling or storage identity. | Exact observable regression comparison remains clean for the affected population. Record p50/p95, peak memory, rewrite/bind counts, and retained bytes. Stop when the identified bottleneck is resolved; defer broad storage redesign without a measured need. |

### Correctness gates

These are mandatory:

- Exact values, types, errors, and recorded observable calls/events for captured cases.
- No unexplained changes to existing verdicts.
- Typed refusals remain failures in headline accounting.
- No mixed authored/result/save frontiers.
- No source or prior destination damage on rejected or failed operations.
- No silent format conversion or feature loss.
- No stale async authority revived by undo or address reuse.
- Native and wasm share the same supported semantics.

Reuse the existing round-trip tests, reader-tolerance fixtures, capture lineage, corpus harness, and verdict path. Report fresh execution separately from the repository measurements supplied here.

### Proposed performance budgets

These are provisional engineering targets, **not correctness criteria or claimed current results**:

- On a recorded reference machine, target ordinary single-cell prepare/publication below a 16 ms native interaction budget and a 50 ms wasm budget for a specified large sparse workbook, excluding recalculation and rendering.
- Eliminate the extra structural-application pass visible today.
- Track memory by unique retained allocations, not only undo-step count.
- Apply configured source/expansion/concurrency limits before uncontrolled growth.
- Keep save and calculation cancellation responsive at measured work boundaries.

Benchmark at least a formula-dense workbook, a sparse workbook with distant cells, many small sheets, shared-formula-heavy files, and large XLSB conversions. Adjust performance targets from those measurements without relaxing semantic gates.

---

## 5. Discriminating experiments against Excel

All experiments below are **[PROPOSED]**. Every predicted outcome is a **hypothesis**, not a measured fact.

### Common protocol

Use fresh harness-owned Excel processes and byte-identical seed copies. Record build/channel, OS, bitness, locale, date system, calculation settings, iteration settings, multithreading settings, enabled-sheet state, add-ins, project permissions, and workbook opening order.

Create the seed in Excel, calculate explicitly, capture it, save, and close. For each branch, record the actual calculation mode after opening.

Capture:

- Raw numeric bits through the existing native capture route, plus value types and errors.
- Formula, Formula2, and R1C1 representations where supported.
- Callback/event sequence, counts, caller, and thread identity where available.
- Array/spill membership and saved package contents.
- `Workbook.Saved`, calculation-state observations, and save/reopen results.

**Do not call `Workbook.Saved` a per-cell calculation dirty flag.** Keep DNA Calc’s exact dirty set separate from Excel observations. `Range.Dirty` changes state; it is not a read-only dirty-set query.

Keep logs outside worksheet cells. Inspect saved packages **before reopening**, then use separate copies for Manual and Automatic reopen. Use actual UI operations for native Undo tests; scripted VBA edits can change the available undo history.

Office.js experiments require Excel’s JS runtime and establish that API’s behavior only.

### E1 — Manual save, before-save edits, and calculation-domain scope

**Seed:** Open target `T` then peer `P`. Each contains `A1=1`, `B1=Audit(A1)`, where `Audit(x)` returns `10*x` and logs externally. Calculate and save caches of 10.

**Branch:** Manual mode; change `T!A1=2`, `P!A1=3`. Starting B1 caches remain 10. Cross CalculateBeforeSave false/true with Save, SaveAs, and SaveCopyAs. Repeat with reversed opening order.

Add a BeforeSave handler that logs, optionally sets `T!A1=4`, and optionally cancels. Repeat with the active target sheet disabled.

| Hypothesis | Expected observable outcome |
|---|---|
| False preserves permitted stale caches | Target cache remains 10; no save-caused Audit invocation. |
| False drops stale caches | Target cache is omitted/invalidated; later open/calculation may invoke Audit. |
| True calculates target only | Target becomes 20; peer remains cached at 10. |
| True calculates the relevant domain | Target becomes 20 and peer 30, with corresponding calls. |
| Handler edits precede the saved model | Saved A1 is 4; calculated B1 is 40 when that calculation occurs. |
| SaveCopyAs bypasses the ordinary event route | No BeforeSave log for that command; its cache behavior is measured independently. |
| Cancellation aborts output but retains handler edits | Destination is not replaced; live A1 remains 4. |

Inspect flags, calcChain, live caches, and event order. The supplied P06 contract already reports some of these distinctions; reproduce that profile before generalizing.

**Design consequence:** determines pin timing, cache policy, and whether save calculation must cross workbook boundaries.

### E2 — Stale array/spill caches and serialized value types

**Seed:** Separate legacy-array and dynamic-array workbooks with `A1=2` and a two-element result `A1*{1;2}`. Calculate and save values 2 and 4. Include independent Boolean, error, empty-text, and representative numeric-bit fixtures from the existing corpus.

**Branch:** Manual, CalculateBeforeSave=false; set A1=3, save without calculating. Use an instrumented array-returning provider on a separate fixture to count reopen invocations.

| Hypothesis | Expected outcome |
|---|---|
| Excel preserves the stale group | Saved caches remain 2 and 4 with coherent membership/extent; no calculation is caused solely by save. |
| Excel drops or invalidates the group | Cache records or flags change accordingly; subsequent calculation produces 3 and 6. |
| Anchor and members follow different rules | Capture the exact asymmetric package and reopen behavior; do not infer it from scalar policy. |
| Numeric serialization preserves the admitted value | Reopened raw bits/type match the captured seed. |
| Serialization normalizes an admitted representation | Bits/type differ in a repeatable build-specific way; record that profile. |

**Design consequence:** determines whether scalar-only permitted-cache handling must be extended and which metadata belongs to one cache group.

### E3 — Copy, fill, cut, axis edits, and partial-range movement

**Seed:** `Data!A1=10`, `A2=20`, `B1=$A$1+A2`; `Watch!A1=Data!B1`. Calculate/save caches of 30.

Run each operation on a fresh clone in Manual mode, capture formulas immediately, then calculate explicitly.

| Operation | Competing hypotheses and expected outcomes |
|---|---|
| Copy B1 → C2 | Relative relocation gives `=$A$1+B3`, value 10 if B3 is blank; literal copying leaves value 30. Watch continues to reference the source under ordinary copy semantics. |
| Cut B1 → C2 | Target-tracking behavior retains A1/A2 dependencies and rewrites Watch to C2, giving 30. Copy-like behavior changes the relative dependency; coordinate-fixed Watch reads the vacated location. |
| Insert row 1 | Referential transformation moves the formula and its dependencies, retaining 30; coordinate-fixed interpretation yields distinguishable formulas/values. |
| Delete a directly referenced cell’s row | Reference invalidation yields an error; coordinate replacement follows the next cell now occupying the location. |

For a partial-range case, seed A1:A3 with 1,2,4 and `SUM(A1:A3)=7`; cut A2 to C2.

- **H1:** reference follows moved members, retaining 7 through an appropriate rewritten reference.
- **H2:** reference remains the original rectangle, yielding 5.
- Other outcomes require their actual formula text and values to be recorded.

Save/reopen each result.

**Design consequence:** rejects a universal rectangle translation or copy-then-clear implementation.

### E4 — Relative names, scope, tables, and 3-D spans

**Seed:** Sheets `First`, `Middle`, `Last`, `Watch`, with A1 values 1,10,100 and `Watch!A1=SUM(First:Last!A1)` cached at 111. Add workbook `Rate=2`, sheet-local `Data!Rate=3`, and formulas exercising both scopes. Create `RelLeft` with B2 active and an accepted relative R1C1 definition. Add table `T` with `Val` values 1,2 and `SUM(T[Val])=3`.

**Branches, all Manual followed by explicit calculation:**

- Move Middle outside the 3-D endpoints.
  - **H1:** order-defined membership produces 101.
  - **H2:** a retained identity set produces 111.

- Reorder/rename Data and save/reopen.
  - **H1:** local name remains attached to Data, yielding 3 there.
  - **H2:** incorrect index/text binding changes its scope or resolution.

- Delete the local Rate.
  - **H1:** resolution falls back to workbook Rate, yielding 2.
  - **H2:** the prior binding remains invalid and produces an error.

- Invoke RelLeft from column A with XFD1=9.
  - **H1:** wrap gives 9.
  - **H2:** off-grid reference fails.
  - **H3:** creation-context anchoring resolves a different cell.

- Rename table column Val and insert a row containing 5.
  - **H1:** references follow column identity and the supported expansion rule, producing 8.
  - **H2:** old-name binding fails or the old extent remains at 3.

Record table auto-expansion options. Test cell copy across the grid edge separately; name wrapping does not prove copy wrapping.

**Design consequence:** defines scope identity, rebinding triggers, old-order requirements, and table expansion semantics.

### E5 — Formula dialect, shared membership, and arrays

**Seed:** A1:A3 contain 11,22,33. On a dynamic-array-capable build:

- Set B2 through Formula to `=A1:A3`.
- Set C2 through Formula2 to the same text.
- Set D2:D4 through FormulaArray to `=A1:A3*2`.
- Fill E1:E3 from `=A1*2`.
- Set G2 through FormulaR1C1 to `=RC[-6]`.

Calculate, save, inspect text/metadata/caches, and reopen.

- **H1 — semantic dialect differs:** B2 intersects to 22; C2 spills 11,22,33.
- **H2 — spelling-only distinction:** both routes have the same evaluation and storage behavior for this case.
- **H3 — R1C1 is an entry channel:** G2 serializes with an equivalent A1 reference and retains 22.

For shared membership, replace the middle member of an explicit shared-formula fixture with scalar 999.

- **H1:** explicit membership permits the hole.
- **H2:** Excel reconstructs, repairs, or refuses it.

Do not assume Excel chooses shared serialization for the small FillDown; construct and label a minimal package fixture if necessary.

Attempt a legacy-array member edit:

- **H1:** refused without mutation.
- **H2:** a broader replacement or other mutation occurs.

For spills, place an obstruction **outside the current extent**, then expand the demanded extent into it.

- **H1:** a spill error occurs while the obstruction remains authored.
- **H2:** extent/output handling differs.

**Design consequence:** establishes formula form/dialect storage and prevents result cells from becoming authored cells accidentally.

### E6 — Opaque references and four levels of fidelity

**Seeds:** Separate workbooks containing:

1. An ordinary worksheet with an embedded chart.
2. A chart sheet.
3. A dialog sheet with a control link.
4. A macro sheet with harmless references, execution disabled.

Each references Data cells containing 10 and 20. Use Excel-produced legacy fixtures on a supported platform.

**Branches:** unrelated scalar/style edit; rename Data; remove Data; insert row 1; reorder; visibility change. Save/reopen.

- **H1:** unrelated edits leave opaque semantics valid; preservation is sufficient.
- **H2:** relevant operations rewrite series/control/macro references.
- **H3:** unchanged opaque bytes leave stale references and incorrect behavior despite repair-free reopen.
- **H4:** a supposedly safe reorder affects an order-dependent reference.

Inspect actual series/control formulas and values, not only hashes or repair dialogs. Compare whole ZIP, uncompressed parts, raw preserved records where promised, semantics, and reopen independently.

**Design consequence:** validates existing refusals and determines coverage beyond preserved chart/dialog sheets.

### E7 — Reader tolerance, repair identity, and untouched save

Use the existing empty-rId, dangling-rId, duplicate-ID, array-inner-empty-formula, array-inner-text-formula, and malformed-sheet control fixtures. Preserve their initial caches exactly.

Open separately through automation normal-load and UI File > Open. Then compare DNA Calc’s repair report and these outputs:

- Untouched source preservation.
- Ordinary loaded-model save.
- Explicit normalized save, if separately exposed.
- Save after one unrelated scalar edit.

| Hypothesis | Expected outcome |
|---|---|
| Preserve original | Output can be byte-identical; Excel repeats the source’s original acceptance/repair behavior. |
| Normalize on ordinary save | Repair-causing records change; subsequent Excel open needs no repair. |
| Partial normalization | Inconsistent IDs/relationships or array records cause another repair or content change. |
| Silent tolerance misclassified | DNA reports repair for a fixture Excel opens silently. |

Record sheet names/order/visibility, internal identity continuity, source IDs, scoped names, dialogs, and normalized records.

**Design consequence:** determines repair disposition and catches the normalized-base/original-bytes gap.

### E8 — XLSB/XLSM conversion, VBA source, and signatures

**Seeds:** Excel-produced xlsx/xlsm/xlsb variants containing names, arrays, stable caches, a modern data bar, and—for macro formats—a project with `F()` returning 7. Keep signed variants separate.

**Branches:** untouched save; calculation-only; view-only; scalar/style/name edit; explicit XML conversion; source edit changing F to 9.

Distinguish Excel behavior from DNA Calc capability:

- **H1:** the original-binary route applies only with no planned rewrites; changed same-format saves refuse.
- **H2:** an authored binary route exists and preserves the edited content. This requires evidence beyond copying the source.
- **H3:** Excel conversion preserves a modern property that DNA’s documented conversion loses.
- **H4:** a source edit is runtime-only: F returns 9 in-session but 7 after reopen.
- **H5:** source write-back is supported: reopened source and execution return 9.

For signatures:

- **H1:** an edit outside the signature’s covered content leaves it valid.
- **H2:** a covered change invalidates or removes it.
- **H3:** signature bytes survive but validation fails.

Inspect content types/extensions, source/module content, project bytes, signature validation, formula caches, and repair logs. Do not change Trust Center settings silently to make a test run.

**Design consequence:** separates original preservation, conversion, authored binary writing, source persistence, and trust claims.

### E9 — Late async completion, structural edits, undo, and save

**Seed:** Controlled async XLL `Slow(x)` returns `10*x`. A10=1, B10=`Slow(A10)`; complete and save cache 10.

**Branch:** Manual; change A10=2; calculate to launch T1; withhold completion. Through UI, insert row 5. In a separate branch, put `=777` at old B10.

- **H1 — transfer:** T1 publishes 20 only to the moved formula, with no replacement call.
- **H2 — restart:** T1 is ignored; a new token supplies 20.
- **H3 — defer:** T1 is ignored and the moved formula remains stale/pending until another calculation.
- **Failure condition:** T1 overwrites unrelated B10=777.

Run separate Undo branches using the actual available undo sequence. If two user actions were made, record both undos rather than pretending the insertion alone is the latest action.

Add Save(false)/Save(true) while T1 is pending, then complete, cancel, or edit.

- **H1:** save waits for a qualifying result.
- **H2:** save uses an allowed stale cache.
- **H3:** save becomes incomplete/cancelled.
- **H4:** Excel serializes interaction and rejects/defers the attempted concurrent edit.

**Design consequence:** determines activation transfer and save waiting behavior. It does not permit address-only admission under any outcome.

### E10 — Office.js handles and synchronization visibility

Use Excel’s actual JS runtime, with a calculated seed and Manual mode.

```javascript
// PROPOSED experiment outline; verify the build's supported API set.
await Excel.run(async context => {
  const r = context.workbook.worksheets
    .getItem("Data").getRange("B1");

  context.trackedObjects.add(r);
  r.load(["address", "values", "formulas"]);
  await context.sync();

  // Harness/UI performs one operation on a fresh branch:
  // row insertion, cut, rename, deletion, or deletion + user Undo.

  r.load(["address", "values", "formulas"]);
  await context.sync();

  // Record returned properties or structured API error.
});
```

- **H1:** the tracked Range follows the referent.
- **H2:** it retains coordinates.
- **H3:** it invalidates with a particular error.
- **H4:** deletion followed by undo revives it.
- **H5:** deletion permanently invalidates that handle.

Separately queue mutation and load in one batch:

- **Queue-time observation hypothesis:** read reflects the earlier state.
- **Execution/synchronization hypothesis:** read reflects the ordered batch’s later state.

Repeat relevant handle cases through COM/VBA without assuming identical policies.

**Design consequence:** supplies handle and batching contracts. It does not expose Excel’s internal scheduling algorithm.

### E11 — Worksheet restrictions, callbacks, and thread ownership

**Seed:** A1=1; a normal macro and a worksheet UDF call the same helper that attempts to set A1=99, with errors logged outside cells. Add BeforeSave/Calculate event logging and native thread-ID logging where available.

Use explicitly enabled and disabled project branches, Manual first, Automatic as control.

- **H1:** normal macro mutation succeeds, worksheet-function mutation is rejected.
- **H2:** worksheet mutation is deferred.
- **H3:** worksheet mutation is admitted immediately.
- **H4:** reentrant callbacks run inline on the owning thread.
- **H5:** they are queued until the outer operation reaches a safe boundary.
- **H6:** thread-safe native functions run on workers while VBA callbacks remain owner-thread-bound.

Capture A1, returned errors, event nesting/order, call counts, and thread IDs. A single observed thread does not prove all routes are serialized.

**Design consequence:** establishes invocation-context restrictions and marshaling. It does not establish that CalcVBA Safe is equivalent to Excel trust policy or that a capability gate is an OS sandbox.

---

## 6. What to stop or delete

1. **Stop treating source baseline and current authored root as duplicate authority.** Keep both; remove current-state reads from baseline objects where they exist.

2. **Retire stale critiques as current defect statements.** The supplied code already has prepared edit machinery, pinned saves, preserved-sheet rename/removal refusal, host repair reporting, and original-binary XLSB preservation.

3. **Remove the repeated structural application after preserving batch semantics.** Instrument first; retain one public revision and one notification boundary.

4. **Eliminate independently interpreted edit semantics in adapters.** JS, VBA, skin, engine, and serializer should consume one accepted authored operation/change description.

5. **Stop using address, text equality, or node ID alone as async authority.** Validate the formula lifetime, command, input frontier, and destination through existing contracts.

6. **Stop presenting DropStaleCaches as automatically Excel-equivalent.** It can change reopen and UDF invocation behavior.

7. **Stop conflating byte preservation, semantic preservation, and repair-free reopen.** State and test each promise separately.

8. **Stop silently blessing documented XLSB conversion loss.** Preserve the original where possible; expose or refuse unsupported fidelity. Defer an authored binary writer until its capability priority justifies the work.

9. **Stop describing preserved macro/chart/dialog sheets as modeled surfaces.** Preserve useful content and refuse unsupported edits honestly.

10. **Stop treating unchanged VBA bytes as proof of supported source write-back or valid signatures.** Keep those contracts separate.

11. **Do not introduce a second evaluator, graph, workbook model, async registry, capture lineage, or verdict path.** Temporary prepared states are legitimate; parallel semantic implementations are not needed.

12. **Defer broad storage rewrites, permanent row/column identities, generalized transformation registries, and a second binary-loading model.** First measure the existing repeated work, rebinding, source expansion, save traversal, and retained snapshots.

13. **Preserve the compatibility evidence.** Keep reader-tolerance fixtures, malformed controls, opaque-part packages, P06 captures, failed/refused cases, and separate saved-cache populations. A limitation becoming an explicit refusal is useful progress, but it remains a failed capability until the requested Excel behavior is delivered.



