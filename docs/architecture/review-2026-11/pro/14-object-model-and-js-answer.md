# 14 — One Excel object model through VBA, COM/XLL and JavaScript

Review date: 12 November 2026.

**DnaCalc pin:** [3b5aa86b3ba47cbd216daec70c5d6586ad520deb](https://github.com/DnaCalc/DnaCalc/tree/3b5aa86b3ba47cbd216daec70c5d6586ad520deb).  
**CalcVBA pin:** [8115aa78135114c1dea79f26cbc12c8a3b00a4cf](https://github.com/DnaCalc/CalcVBA/tree/8115aa78135114c1dea79f26cbc12c8a3b00a4cf).

This review uses only the supplied packet. No repository checkout, external documentation, build, benchmark or Excel execution was used. Source citations below name the exact pinned source files and original source lines in that packet; a citation to an excerpt does not imply inspection of its omitted surroundings. All unprefixed repository paths are DnaCalc-relative; `CalcVBA/` denotes the separately pinned repository.

**Evidence labels:** **[CODE]** identifies behavior or declarations visible in supplied code; **[DESIGN]** identifies design or tracker statements; **[EARLIER-REVIEW]** identifies previous recommendations; **[MEASURED-EXCEL]** identifies reported Excel evidence, limited to its recorded cases and builds; **[INFERENCE]** identifies deductions or risks; **[PROPOSED]** identifies this review's design, implementation work and experiment predictions. Comments in code establish intended contracts, not proof of their execution.

## 1. Verdict on today's design, with evidence

### 1.1 Main decision

**[PROPOSED] Extend the real workbook host. Put a small application coordinator immediately above `HostSession`, preferably in `calc-host` initially, and make VBA, native COM/XLL and JavaScript projections of its operations and identities.** Keep workbook edits, calculation and persistence on the current host/engine/document routes. Keep CalcVBA responsible for the language, invocation and ABI mechanisms. Keep OneCalc's values-only profile.

**[INFERENCE] The implementation has useful pieces of this design, but the packet does not establish a unified Excel application object model.** Its strongest evidence is the existing workbook-edit route, not the local COM object hierarchy. It would be a mistake either to discard that route or to declare the local `Book1` shell the canonical application.

There are three materially different surfaces:

1. **[CODE] The VBA read/UDF projection.** `SpreadsheetContext` supplies caller and reference operations, reads, worksheet functions and volatility. Default active-sheet and selection operations refuse when no corresponding host view exists. `event_view` explicitly recognizes that a reentered UDF must not inherit a surrounding document event's Target. This is useful invocation machinery. It does not establish complete workbook identity, writable automation or Excel's read-freshness rules. See `crates/calc-vba/src/runtime/spreadsheet.rs:58–101`.
2. **[CODE] The real workbook event/write route.** `write_reference` validates a rectangular `Value2` assignment and submits `dispatch_from(CommandIngress::Automation, SkinIntent::Edit)`. Its calculation method routes dirty/full/rebuild/range requests into existing host intents. Therefore, describing *all* VBA hosting as read-only would be wrong. See `crates/calc-host-vba/src/lib.rs:455–509,534–553`.
3. **[CODE]/[DESIGN] The native compatibility shell.** Native objects have reference counting, a connection flag and owner-thread checks; their state contains a local workbook and worksheet. The README explicitly says the workbook collection describes one local `Book1` shell, rather than arbitrary application documents. These are useful compatibility and lifecycle mechanisms, but are not evidence that COM clients operate on the real `HostSession`. See `crates/calc-com-host/src/projection.cpp:29–89` and `crates/calc-com-host/README.md:30–35`.

### 1.2 Assess the product routes, not just the types

| Route | Evidence supplied | Verdict and missing connection |
|---|---|---|
| OneCalc formula/add-in execution | **[DESIGN]** Owner policy and reported acceptance in `.beads/issues.jsonl:524`; current Caller/ThisCell refusal in `projection.cpp:382–409` | Keep the values-only boundary. The reported 350 supported cases plus three DNA Calc-only cases, and the 109 simple-UDF cases on both routes, are reported checks, not fresh validation here. |
| Workbook VBA event → worksheet edit → host | **[CODE]** `calc-host-vba/src/lib.rs:455–509` and `calc-host/src/events.rs:25–45` | This is the right route to extend. Input validation and host ingress are visible. Bootstrapping, the complete dispatcher, exact event order, final skin delivery and save/reopen are not all shown. |
| VBA reference read → evaluator/host | **[CODE]** `calc-vba/src/runtime/spreadsheet.rs:87–100` | Preserve the per-invocation context. Whether a callback observes a cache, requests evaluation, and/or records a dependency needs separate evidence. |
| Native add-in → Application/Workbook/Workbooks | **[DESIGN]** `calc-com-host/README.md:30–41`; **[CODE]** local state at `projection.cpp:29–48` | Existing limited plugin compatibility is valuable. A real-workbook operation through this route is not demonstrated by the excerpts. |
| Host event → native sink | **[CODE]** `projection.cpp:603–614` | There are SheetChange and AfterCalculate emitters. The shown SheetChange bridge carries one row/column and a local worksheet; it cannot express the real host's arbitrary `Vec<GridArea>` Target as written. |
| Host publication → skin | **[CODE]** `calc-host/src/session.rs:83–129` | Keep the separation of authored revisions, calculation publications, sequence numbers and reset deltas. These types do not prove renderer integration, reliable transport or WebGPU rendering. |
| JavaScript add-in → workbook | No working JS object-model route is supplied | Treat the entire proposed JS projection as new work. Familiar Office.js syntax does not establish Office.js compatibility. |

Paths abbreviated in this table retain their `crates/` prefix where applicable.

### 1.3 Keep these decisions

**[PROPOSED] Keep one semantic implementation for edits and calculation.** The existing Automation ingress is an asset. Keep its supported rectangle validation, existing calculation scopes and explicit refusals while adapters converge. Do not add a COM worksheet store or a JS workbook replica.

**[CODE] Keep the distinction between authored state and calculated publication.** `HostDelta` already distinguishes `Committed`, `Recalculated` and `RecalcFailed` and carries the applicable revision/publication information. Manual-mode cache observations and failed calculations need this distinction. See `crates/calc-host/src/session.rs:95–125`.

**[CODE]/[DESIGN] Keep portable typed objects separate from native Automation.** CalcVBA's `ObjectHeader` explicitly says its typed slot table is a separate versioned ABI, despite having a runtime IUnknown prefix. The hosting contract also keeps native interfaces on their owning apartment and says that `Arc` does not authorize cross-thread execution. See `CalcVBA/crates/calcvba-host-api/src/object.rs:29–41` and `CalcVBA/docs/spec/HOSTING_AND_EARLY_BINDING.md:33–45`.

**[CODE] Keep grants, capabilities and nested policy.** Off/Safe/Unguarded determine open-time execution and map to no policy/Safe/Full respectively. Explicit per-file enable/unblock currently grants Full. A nested Safe scope remains Safe and shares the audit handle. These are separate from worksheet-function write permissions. See `crates/calc-macro-security/src/lib.rs:36–45,77–84`, `crates/calc-host-vba/src/trust.rs:52–76` and `crates/calc-vba/src/policy.rs:40–59`.

### 1.4 Reconcile history with the pin

**[DESIGN] The removal of the OneCalc pseudo object model is intentional and should stand.** The policy in `.beads/issues.jsonl:524` and `docs/architecture/XLL_COM_HOSTING.md:1333–1339` puts workbook-dependent capability in DNA Calc. Restore the useful evidence, not the fake workbook.

**[MEASURED-EXCEL] The historical X19 work reports selected Range/Union/Caller metadata and early-/late-bound native-client comparisons.** It also records the hidden-LCID mistake that crashed the initial `get_Caller` implementation. These observations justify ABI fixtures and metadata-driven lowering. They do not demonstrate current full Range support: the same record explicitly excluded Value, Value2, Cells, Item, Areas and several caller shapes. See `docs/architecture/XLL_COM_HOSTING.md:1342–1361`.

**[EARLIER-REVIEW] Stable references and generated adapters remain good recommendations.** The October answer separates persistent identity from the current authorized reader and separates coordinate equality from COM identity. It recommends generated descriptions and adapter conformance. These are recommendations, not evidence that those changes have been implemented. See `docs/architecture/review-2026-10/pro/2-vba-hosting-answer.md:461–504`.

**[DESIGN]/[INFERENCE] The own-GUID wording needs an explicit update.** `docs/architecture/SYNTHESIS.md:241–245` requires own activation/type identities and allows Excel IIDs only for a complete contract. The packet describes a narrower selected-member compatibility practice. Section 3 makes a deliberate decision about that exception; limited PIA success must not silently become a claim of a complete Excel interface.

### 1.5 What the reported numbers do—and do not—establish

| Population | Reported result | Evidence and limit |
|---|---:|---|
| Oracle formula-cell corpus | 76,903 / 78,264 pass; 1,361 fail | **[MEASURED-EXCEL]** `docs/research/gamma-family-2026-11-12.md:344–348`. The denominator is formula cells, not retained report-file rows. |
| Full bank | 3,895,333 / 3,910,795 pass; 15,462 fail | **[MEASURED-EXCEL]** Same file `305–311`. Unread and excluded rows remain failures; reported comparisons concern that bank and its earlier baseline. |
| Live capture batch 1 | 130,849 / 134,577 pass | **[MEASURED-EXCEL]** `docs/plan/capture-batch-1-results.md:24–39`. Only the 289 captured files contribute; the packet says 486 were selected. This is not a rate for all selected files. |
| Enron saved caches | 12,961,186 / 19,666,149 saved-value agreements; zero live passes | **[DESIGN]**, reported saved-cache measurement in `docs/plan/enron-saved-cache-triage.md:8–21,35–56`. The appendix labels the totals [MEASURED-EXCEL], but their explicit provenance remains unknown-freshness saved-cache comparison, not live Excel capture. |

None of these populations establishes object lifetime, event order, COM ABI coverage or JS behavior. Reuse their capture and verdict machinery; do not add the populations together or use their rates as an architectural acceptance gate.

## 2. Specific defects and risks, ranked

**[PROPOSED] Priority definitions:** P1 blocks a trustworthy full-host route or risks wrong observable effects; P2 is bounded missing capability or a design/coverage risk. There is no basis in this packet to declare a currently reproduced memory-corruption defect. Historical crashes remain historical.

| Severity | Exact pinned file:line | Trigger → observable consequence | Status | Smallest reproducer or evidence still needed |
|---|---|---|---|---|
| P1 | `crates/calc-com-host/README.md:30–35`; `crates/calc-com-host/src/projection.cpp:38–42` | Full application opens two books, but clients are attached to the local shell → wrong workbook enumeration/identity if used as the full-host model. | **[CODE]/[DESIGN]** local scope demonstrated; full-host misrouting is a risk, not a proven current product regression. | Open A and B; query names/count; read different A1 values through native COM, VBA and UI. Need actual composition/attachment route. |
| P1 | `crates/calc-com-host/src/projection.cpp:382–389,409` | Same Caller/ThisCell dispatch with versus without `EXCEPINFO*` → `DISP_E_EXCEPTION` versus `DISP_E_MEMBERNOTFOUND`. | **[CODE]** demonstrated fault-class inconsistency. Excel equivalence not measured. | Direct `IDispatch::Invoke` twice with identical request and nullable diagnostic destination. Preserve the needs-workbook refusal while repairing lowering. |
| P1 | `crates/calc-host-vba/src/lib.rs:506–527` | An edit dispatches, then calculation reports Failed/Waiting/Cancelled/unfinished → the setter returns an error whose authored effects are unclear to its caller. | **[CODE]** error-after-dispatch; **[INFERENCE]** retry/double-effect risk. No rollback defect proved. | Hold an async provider; write; inspect receipt, current input, events and save/reopen. Repeat event error and cancellation. |
| P1 | `crates/calc-vba/src/runtime/spreadsheet.rs:205–219,360–387` | More distinct objects than the 4,096-entry table admits → typed object-limit refusal. | **[CODE]** explicit table limit. Not proof of an unreclaimed cumulative lifetime leak. | Retain >4,096 distinct ranges; separately create/release them in a loop. Need teardown/cleanup and live-entry counts. |
| P1 | `crates/calc-host/src/events.rs:13–16`; `crates/calc-com-host/src/projection.cpp:603–611` | Rectangle or multi-area edit, especially in another workbook → shown native bridge cannot express source identity plus full Target. | **[CODE]** representational mismatch for this bridge. | Edit B2:C3 and disjoint areas; compare native/VBA workbook, sheet, Areas, Address and Count. |
| P1 | `crates/calc-vba/src/runtime/spreadsheet.rs:95–98` | UDF reads a dirty formula via an object → accidental conflation of cache observation, dependency registration and demand evaluation. | **[INFERENCE]** semantic risk; actual readers are omitted. | Seed stale cache; calculate only the reader; then dirty/recalculate its source without explicit arguments. Record values and callback counts separately. |
| P1 | `crates/calc-host-vba/src/lib.rs:506–509,548–549`; `crates/calc-host/src/events.rs:25–28` | Dispatch reenters an event/UDF while session or graph state remains mutably borrowed → potential panic/deadlock or forbidden nested effect. | **[INFERENCE]**; omitted lending/executor logic might already prevent it. | Outer edit → Change → nested edit → calculation → VBA callback, with borrow/lock and thread trace. |
| P1 | `crates/calc-vba/src/runtime/spreadsheet.rs:205–217,389–396`; `crates/calc-com-host/src/projection.cpp:29–68` | Retain a reference through close/reopen, rename or structural edit → no shared lifetime/identity contract is established by supplied excerpts. | **[INFERENCE]** integration prerequisite, not proof that omitted Range fields lack generations. | Retain in an observer add-in; close/reopen same path, delete/recreate same-named sheet, compare all surfaces. |
| P1 | `CalcVBA/crates/calcvba-hal/src/traits.rs:106–118`; `crates/calc-vba/src/policy.rs:40–59` | New cached/queued compile, binding or native dispatch route omits policy scope → permissive base hook or unrelated TLS may be used. | **[INFERENCE]** coverage risk, not an established bypass. | Safe→Full nesting; retained external object after downgrade; compile/load and dispatch audits; deferred continuation. |
| P2 | `CalcVBA/crates/calcvba-hal/src/capability.rs:19–22` | A file handle opened under Full remains reachable in Safe → unlogged handle operations might retain authority despite denied future opens. | **[INFERENCE]** conditional resource-scope risk, not a proven bypass. | Full Open → nested/downgraded Safe Get/Print#/EOF; inspect handle visibility and enforcement. |
| P2 | `crates/calc-host-vba/src/lib.rs:455–470`; `crates/calc-vba/src/runtime/spreadsheet.rs:220–225` | Multi-area assignment, array/target mismatch, large materialization or format-sensitive Value → explicit incomplete behavior. | **[CODE]** refusals/limits. They are failures where the requested Excel operation succeeds. | Compare scalar versus 1×1/undersized arrays, disjoint targets, Date/Currency Value versus Value2. |
| P2 | `crates/calc-host-vba/src/lib.rs:537–546` | Extending the local `Full` command into an Application API without revisiting scope → potential omission of other open workbooks. | **[CODE]** current full mapping is workbook scope; wrong public Application binding is not proved. | Two dirty workbooks; compare Range, Worksheet and Application calculate commands and callback counts. |
| P2 | `crates/calc-com-host/src/projection.cpp:37,600–614`; `crates/calc-host/src/events.rs:11–45` | Separate adapters own event enablement/completion → potentially divergent suppression or duplicate/early AfterCalculate. | **[INFERENCE]** common wiring not established. | Toggle through one surface; mutate through another; include nested handler and pending provider. |
| P2 | `docs/architecture/SYNTHESIS.md:241–245`; `docs/architecture/XLL_COM_HOSTING.md:1357–1361` | Treat selected Excel-IID slots as a complete interface or reconstruct signatures from DISPIDs → unsafe ABI or false compatibility claim. | **[DESIGN]** policy mismatch; **[MEASURED-EXCEL]** historical LCID failure; no current unfixed crash established. | Preserve PIA/dynamic fixture; inspect generated ABI manifest and all advertised slots; round-trip type library. |
| P2 | `crates/calc-vba/src/runtime/spreadsheet.rs:822–840`; `CalcVBA/crates/calcvba-hal/src/capability.rs:571–578` | Treat “ComActivationDispatch supported” as arbitrary COM authorization → misleading capability surface. | **[INFERENCE]** facility availability differs from permission and target kind. | Exact host class, unknown class, GetObject moniker and foreign member invocation under Safe. |

**Evidence boundaries:** the omitted `fire` body prevents a claim that native event arguments are reversed. The omitted `retain_office_owner` and teardown code prevent a proven refcount-leak claim. Checked monotonic compatibility IDs at `spreadsheet.rs:371–379` do not demonstrate recycled-ID collisions. The calculation method is cut at `calc-host-vba/src/lib.rs:553`, so the excerpt cannot establish how its subsequent progress loop behaves. No JS defect can be attributed to nonexistent supplied JS code; there is a coverage gap and a design task.



## 3. Recommended target design and the smallest migration

Everything in this section is **[PROPOSED]** unless a narrower evidence label is attached.

### 3.1 The ownership boundary

Implement `calc-host::application` and `calc-host::automation` as small extensions around the existing session. A separately packaged `calc-object-model` is justified later only for reusable descriptors/generated bindings or dependency hygiene. It must not become another workbook implementation.

| State or behavior | Single authority | What the projections do |
|---|---|---|
| Open workbook incarnations; active workbook/window/selection; application settings; root object lifetimes | Full application coordinator in `calc-host` | Resolve identity and submit operations. The UI supplies selection changes through this authority. |
| Authored cells, formulas, formats, sheets and names | Canonical `calc-doc-model` state, edited through the existing host transaction path | Read metadata or request edits. Never maintain shadow cells or independently mutate document objects. |
| Commit/undo boundaries, authored revision, save preparation and semantic event order | `calc-host::HostSession` and its application coordinator for cross-workbook scope | Invoke the same commands; preserve their receipts and faults. |
| Dependencies, dirty state, evaluation, pending providers, calculation publications | `calc-engine`, using the existing evaluator/function services | Supply callback observations and consume calculation outcomes. Never construct another graph. |
| Package decoding/encoding and saved-cache provenance | `calc-doc` | Call the existing open/save path, including its refusals. |
| VBA execution, project globals, call ABI and language errors | CalcVBA, composed through `calc-vba`/`calc-host-vba` | Bind host-owned objects and callbacks; no Excel sheet/name/structural semantics enter CalcVBA. |
| Native object wrappers, COM transport, XLL callback marshalling | `calc-com-host` / `calc-xll-host` | Translate identities and types, enforce apartment/entry rules, call the host. |
| JS queue, path bookkeeping and loaded property snapshots | JS adapter | Retain transport/client state only; the host remains authoritative. |
| Subscription deltas and their semantic revision/publication stamps | Existing `calc-host` projection | Skins retain disposable mirrors and rendering state. |

The target has **one canonical dependency authority for the admitted calculation domain**. Registering a workbook with the application must not clone its graph for COM, JS or a skin. The packet does not establish today's cross-workbook engine composition. Require the graph review to supply cross-workbook routing through that authority; until then, explicitly limit cross-workbook calculation rather than quietly constructing a second link graph.

Begin with a registry containing **one actual open workbook**. That delivers useful capability without requiring every Excel collection and window operation. “One real workbook currently registered” is a truthful collection state; a fabricated `Book1` that is unrelated to the session is not an acceptable full-host substitute.

### 3.2 One request vocabulary

Retain `dispatch_from` and the existing edit/calculation intents. If the name `SkinIntent` becomes misleading, rename or alias it to a host command type while keeping one executor. Do not leave an old skin executor and add a second automation executor with subtly different effects.

Add typed queries alongside those commands:

~~~rust
// PROPOSED shape; extend existing IDs/value/error types wherever possible.
enum HostQuery {
    ResolveMember { object: ObjectKey, member: MemberId, args: Arguments },
    ReadValues { reference: ReferenceKey, kind: ValueReadKind },
    ReadFormula { reference: ReferenceKey, dialect: FormulaProperty },
    DescribeReference { reference: ReferenceKey },
}

enum ValueReadKind { Value, Value2 }
enum FormulaProperty { Formula, Formula2 }

struct InvocationAccess {
    invocation: InvocationId,
    parent: Option<InvocationId>,
    origin: ExecutionOrigin,
    entry: EntryKind,             // Udf, Macro, Event, Automation, JsBatch
    principal: PrincipalLease,
    policy: EffectivePolicyLease,
    view: ObservationView,
    dependency_observer: Option<DependencyObserver>,
}

enum ExecutionOrigin {
    ValuesOnlyHost,
    Application(ApplicationId),
    Workbook(WorkbookIncarnation),
}
~~~

This sketch is not a demand for a universal reflective interpreter. Known members can call generated typed entry points directly. The descriptor maps each public member to the *same* semantic operation for native early binding, portable early binding, late dispatch and JS.

A query's view and the choice to register dependencies are independent. A reference read might observe a cached value and still be recorded as a read; demand evaluation is another decision. Give the engine an explicit observation contract instead of deriving semantics from whether an adapter happened to call `read_range` or `read_argument_range`.

### 3.3 Persistent identity, lifetime and references

Use existing workbook/sheet/name IDs where available, adding the missing lifetime distinctions once at the host boundary:

~~~rust
// PROPOSED; names illustrate distinctions, not a parallel identity framework.
struct WorkbookIncarnation { workbook: WorkbookId, open_generation: u64 }
struct SheetKey { book: WorkbookIncarnation, sheet: SheetId, generation: u64 }
enum NameScope { Workbook(WorkbookIncarnation), Worksheet(SheetKey) }
struct NameKey { scope: NameScope, name: NameId, generation: u64 }

enum ObjectKey {
    Application(ApplicationId),
    Workbook(WorkbookIncarnation),
    Worksheet(SheetKey),
    Name(NameKey),
    Range(ReferenceKey),
    Collection(CollectionKey),
}

struct ReferenceKey { book: WorkbookIncarnation, slot: u32, generation: u32 }
struct ResolvedReference {
    at: AuthoredRevision,
    areas: AreaSet, // retain observable area ordering; never a bounding rectangle
}
~~~

**Invariants:**

- **A file path is not an open workbook identity.** A different open of the same file gets a different incarnation. Save As normally changes file metadata rather than inventing another live workbook; make that behavior a captured compatibility case.
- **A sheet name/index is a lookup, not identity.** Rename and reorder cannot silently retarget a held object. Deleted/recreated same-named sheets cannot accidentally inherit old handles.
- **A Name has scope and identity.** A workbook Name, a sheet-local Name and the Range returned by `RefersToRange` are different things. Names can denote constants or expressions as well as references; `RefersToRange` must not fabricate a rectangle for a non-reference.
- **A retained Range holds reference identity, never a borrowed evaluator reader or a permanent permission grant.** Each invocation resolves it through its current authorized host access.
- **Range identity, equal extents and wrapper identity are separate.** Two acquisitions describing equal cells need not be the same COM object. All interfaces of one native object retain the proper controlling IUnknown identity. JS `===` compares wrappers, not workbook references.
- **An ordinary value edit does not invalidate every object.** A revision is an observation/version stamp. Incarnation and generation are lifetime checks. Requiring an object's creation revision to equal the current revision would make persistent ranges unusable.
- **Closing disconnects the live workbook authority.** Keep enough tombstone/generation state to prevent retargeting. Whether Excel leaves particular metadata readable or permits a detached view is measured in E1; do not universalize “every member now throws.” Old objects must never silently become objects in an unrelated reopened workbook.
- **Undo creates a new authored revision.** It does not rewind revision counters or justify reusing object generations. Whether an old deleted-sheet/range object revives after undo is a captured rule; the host representation must be able to distinguish deliberate revival from accidental ID reuse.

The structural transformation owner already needed by formula/document edits must also transform live reference descriptors. Do not implement row insertion separately in C++, Rust VBA and JS. Start by updating live descriptors at structural commit using that same transform. Add an index only if measurements show that scanning live references is expensive; there is no reason to allocate a tracking node per cell.

Names require the same discipline: changing `RefersTo` changes the authored Name definition; a newly requested range resolves that definition. Whether a previously returned Range follows that later redefinition is E2, not an implementation convenience.

**Multi-area behavior:** retain the constituent areas and their observable order. Union, overlap, duplicate removal, Areas enumeration, first-area Rows/Columns behavior and Value/Value2 assignment are separate operations with captured rules. The historical metadata evidence supports selected first-area behavior, not wholesale normalization semantics. Do not convert disjoint areas to their bounding rectangle.

**Collections:** implement Count, Item/default member, native enumeration and portable iteration over real registry objects. Keep numeric indexing, name lookup, ordering and mutation-during-enumeration behavior explicit. A host iterator can back both `IEnumVARIANT` and portable iteration; the existing eager `Vec<Variant>` enumeration remains an adapter option only where it preserves measured behavior. Do not assume snapshot versus live enumeration from that trait signature.

**Release:** runtime objects and tracked JS objects retain leases on host entries. Native wrappers retain/release them, and reset/unload releases the corresponding project leases. Free entries when their live leases end; use generations to reject stale transport handles. Measure live objects separately from allocator-reserved memory. A resource budget is still necessary, but “4,096 entries forever” must not be the full application's lifetime design. Until replacement lands, keep the explicit limit and report affected cases as refused.

### 3.4 What survives of the current VBA and COM code

`SpreadsheetContext` survives as an invocation-scoped *view adapter*. Its reads, caller, metadata, worksheet functions, volatility and event/UDF distinction remain useful. Extend/delegate it to the host's semantic services; do not attach workbook ownership to scoped TLS.

`SpreadsheetHost` survives as the CalcVBA host-service adapter and carrier table. Its internal compatibility IDs can map to canonical host keys while migration proceeds. They must stop being an independent authority for workbook/range meaning. Preserve its distinction between host-owned objects and foreign COM delegation at `crates/calc-vba/src/runtime/spreadsheet.rs:353–357`.

The event host's rectangular `Value2` validation and Automation ingress survive. Move shared conversion/validation into the common host operation only when another adapter needs it, then make the event route delegate to it. Do not maintain two setters.

The native projection's reference counting, disconnection/thread checks, connection-point behavior and supported plugin integration survive. Replace the full-host backend's local workbook/worksheet fields with canonical keys and a host gateway. The OneCalc backend retains only its intentionally admitted shell/plugin services and needs-workbook refusals. This is one projection mechanism with explicit profiles, not an excuse to grow two workbook models.

### 3.5 Native ABI, portable tables, type metadata and errors

Use a single descriptor for supported semantic members, generating small adapters as needed. It must also describe the complete physical layout/signatures of every advertised native interface, including unsupported members' refusal slots. It should distinguish:

| Descriptor information | Why it matters |
|---|---|
| Semantic member ID; interface identity; method/get/put/putref | A property setter and getter are not interchangeable calls. |
| DISPID and native vtable slot as separate fields | A dispatch identifier is not a slot number. |
| Logical parameters and complete lowered wire signature | Hidden LCID, HRESULT/result pointers, target pointer width and calling convention belong to lowering. |
| Parameter direction, ByRef alias groups and writeback | Two arguments can alias the same VBA variable. Copyback order and failure behavior must not be invented by a generic vector adapter. |
| Missing/default/Empty/Null/Error distinctions | Optional omission is not an empty cell or JS null. |
| String/object ownership; SAFEARRAY shape, element types and bounds | The same cell values can still be transported incorrectly. |
| Default members, source interfaces, enumeration and type names | TypeName, default coercion, WithEvents and collections are observable. |
| Effects/capabilities and fault mapping | Every invocation reaches the same admission and error rules. |

**Portable host tables remain portable host tables.** Do not cast `ObjectHeader` into a native Excel dual interface. Native Windows wrappers implement the genuine target ABI. Portable typed entry points and wasm handles call the same semantic operation through their own versioned ABI.

**Early binding must remain early binding.** The compiler-selected native IID/slot/signature is authoritative. An unsuccessful dual-vtable call must not silently retry through `IDispatch`; that could run an operation twice and conceal a binding error. Pure dispinterfaces use an explicit Invoke plan. Dynamic calls on DNA-owned objects use the descriptor's dynamic path. Foreign dynamic COM remains a separately admitted capability/profile, preserving today's OneCalc restrictions until explicitly expanded. This follows the supplied CalcVBA contract at `CalcVBA/docs/spec/HOSTING_AND_EARLY_BINDING.md:33–38` and `CalcVBA/crates/calcvba-hal/src/traits.rs:548–549`.

Round-trip generated IDL/type-library metadata through CalcVBA's importer for the admitted slice. Test actual x86 and x64 lowering where supported, including hidden parameters and ByRef cancel. Keep generic marshalling for third-party signatures as an explicit fallback tier at binding time, not an invisible retry after execution.

**GUID decision:** keep DNA-owned LIBID/CLSID/ProgID identities and portable interfaces as the canonical API. Do not register as Excel or replace Excel activation. Preserve the existing selected-member Excel-IID compatibility facade for supported native clients, but explicitly revise the earlier “complete contract only” wording to permit this **limited compatibility facade**.

That exception has strict meaning: every advertised slot must have a known, correct physical ABI and intentional support/refusal behavior. A catch-all stub with the wrong signature is unsafe even when it only returns an error. If safe complete slot layout cannot be established for an advertised interface, do not advertise it. ABI coverage does not equal complete Excel semantics; refused members still fail compatibility cases. This decision preserves useful native clients without pretending the whole Excel interface exists.

**Faults:** enrich the existing error/receipt path rather than inventing a second verdict system. Carry semantic fault kind, member/argument information, source identity, capability/refusal detail, and committed-state information where relevant. Lower to VBA Err, HRESULT/EXCEPINFO/IErrorInfo or JS rejection at the boundary. Preserve underlying error details in the capture record.

A successful result containing a worksheet `CVErr` is different from a failing host method. `On Error` may catch a VBA exception but must not erase a recorded not-modelled refusal from the evaluator verdict. Keep the existing `SpreadsheetContext::refuse` intention at `spreadsheet.rs:83–85`.

Repair the null-EXCEPINFO split by generating the semantic refusal independently of whether the caller requests diagnostic text; the exact native status/error-info mapping must be captured and consistent with the admitted facade. Do not remove the OneCalc refusal to make its transport look successful.

### 3.6 Reads, formula setters and calculation

**Cached object-model reads:** a macro `Range.Value2` read uses the current host-visible authored state plus the applicable calculated cache. A literal just written must be visible to a subsequent permitted read. A formula cache may remain old under manual calculation. Do not pin an entire macro to an invocation-start cell snapshot.

**UDF reads:** the engine supplies the admissible view for the calculation invocation. Carry separately: what value was observed, whether a dependency was registered, and whether execution was delayed/retried. A “cleaner” dynamically tracked reader would be an Excel divergence if Excel actually leaves an undeclared read untracked in that case. E3 decides this per entry/callback category.

Allowed UDF signals such as Volatile remain engine/callable metadata operations with their measured lifetime, not worksheet edits. Preserve the existing `set_volatile` route and OneCalc volatility behavior when applying the new effect checks.

**Value versus Value2:** canonical cell values remain the existing calculation values. Format-dependent Date/Currency conversion belongs to the shared host semantic service, with access to canonical number-format metadata; individual VBA/native/JS adapters transport its results. It must not change numeric kernels. The current Value refusal remains until this conversion is modeled and captured.

**Formula versus Formula2:** retain the setter's semantic intent through the existing formula binding/edit interface. Add a formula-entry mode if none exists; do not reduce both properties to “set this string.” Legacy implicit-intersection intent and dynamic-array intent, getter normalization, spill errors and save representation require E7. Unsupported modes receive a refusal before mutation.

**WorksheetFunction versus Evaluate:** WorksheetFunction uses the existing function/evaluator service with object-model argument coercion and exception projection. Evaluate invokes the same parser/binder/evaluator in a specified workbook/sheet/name/base-cell context. It can yield references or error values according to the captured contract. Neither gets a second evaluator. An error-returning Evaluate result must not automatically inherit WorksheetFunction's exception behavior.

**Calculation scope:** distinguish Range, Worksheet, Workbook and Application, plus dirty/full/rebuild selection. Application scope uses the live workbook registry and engine's calculation authority. Preserve the current local full-workbook route while adding genuine application scope; do not silently change an existing workbook request to global calculation.

**Macro commands and Application.Run:** resolve a registered callable, its project/generation and its admitted entry kind. A VBA declaration named `Function` does not by itself make an invocation a worksheet UDF; the scheduler/call boundary determines that role. A synchronous nested Run retains the outer restrictions and cannot launder UDF write permission. An explicitly admitted queued macro executes later at a command boundary with a fresh invocation and current grant check. Measure special Excel entry-category behavior in E10 before expanding that admission. Preserve OneCalc's existing `Application.Run`/`SyncMacro_<guid>` transport and its refusal of workbook edits. The packet records that transport and assigns an edit-aware command entry to DNA Calc in `docs/architecture/XLL_COM_HOSTING.md:1361–1364`; it does not establish that the full command entry already exists.

### 3.7 Mutation, calculation, events, save and skin publication

Use the existing dispatch route with an outcome that tells the truth:

~~~rust
// PROPOSED extension of the current receipt, not a second transaction system.
struct OperationOutcome {
    authored: AuthoredEffect,       // NotApplied | Applied { revision }
    calculation: CalcOutcome,      // NotRequested | Complete | Pending | Failed | Cancelled
    publications: PublicationSummary,
    events: EventOutcome,
    fault: Option<HostFault>,
}
~~~

A failing operation can itself have authored effects, including effects from a nested handler. “The successful prefix committed” is insufficient if the failing setter committed and then its calculation failed.

For a currently supported rectangular edit:

1. Resolve the target and check incarnation, current authority, invocation effects and supported shape.
2. Validate all values before dispatch, preserving today's behavior.
3. Commit one authored edit through the existing transaction path; update structural references if applicable; invalidate dependencies.
4. Run Change/calculation phases at the captured boundaries. Migration initially preserves the existing dispatcher. **Change before automatic calculation is a hypothesis for E4, not a fact established by the packet.**
5. Publish admitted calculation results through the existing publication mechanism. Failed/cancelled work cannot masquerade as a successful publication.
6. Deliver calculation events and application AfterCalculate only when their captured completion conditions hold, including relevant pending providers and handler-created work.
7. Return the semantic outcome; lower the visible success/error to the caller's API.

The host event record contains source workbook/sheet keys, full Target areas, causal invocation/command identity and relevant revision/publication. `WorksheetChange` represents authored change targets, not every dependent formula whose result changed.

Use one event authority for VBA document/WithEvents handlers, native connection points and JS notifications. Extend the real `WorkbookEvent` route with application events as needed. Do not independently synthesize AfterCalculate in each adapter whenever a local method returns.

`EnableEvents` is application state checked by that event authority. Suppressing events must not suppress edits, dependency invalidation or skin updates. Whether disabled events are dropped or later replayed, and precisely when the flag is sampled, are E4 capture questions. Do not automatically suppress nested events to avoid implementation difficulty.

Keep synchronous cancel events synchronous. BeforeSave can change authored state and set Cancel before the save snapshot is pinned. File write cancellation and handler-write rollback are different questions; E5 determines the observed result. Native ByRef copyback must finish before the initiating call resumes.

The existing `HostDelta` remains the skin's source. Stamp authored changes, calculation publications, failures and event observations consistently. A transport may coalesce repaint work, but it must preserve subscription gap/reset semantics and must not fabricate a newer formula result for a newer authored revision. Manual-mode stale caches are legitimate observable state. Snapshot recovery after a sequence gap must come from the host.

The desktop WebView2 bridge and browser/worker bridge carry the same serialized semantic data. Layout, clipping, glyph placement and render resources can remain in a WebGPU-friendly disposable IR; nothing in this recommendation or in the supplied delta types establishes an implemented WebGPU renderer.

### 3.8 Owner-thread reentry and waits

VBA execution, callbacks and native apartment objects stay on their owning thread. `Mutex` and `Arc` are storage/ownership mechanisms, not permission to execute the runtime anywhere.

Before entering user code, end mutable field borrows and release graph/session locks that nested host work might need. Retain explicit continuation state and a scoped host-access token. Do not keep a `RefMut<HostSession>` alive across an arbitrary callback and then recursively borrow the same cell.

A required execution trace is:

1. An engine worker needs VBA and suspends that task without retaining the graph lock needed by callbacks.
2. The owner thread enters a UDF invocation with restricted effects; callbacks read through its authorized view.
3. The owner returns the typed result and observed dependencies.
4. The engine resumes, checks eligibility and publishes only if the work is still eligible.

A synchronous macro or COM caller waiting for calculation must service admitted owner-thread callbacks/STA messages through this scheduler. Blindly joining the engine worker deadlocks when that worker is waiting for the same VBA thread. Arbitrarily pumping every pending application command is also wrong: it can admit an unrelated edit into an incompatible suspended calculation.

Support explicit reentry categories: nested event commands, callback queries, required provider completions and platform message servicing. `DoEvents` is an explicit broader reentry point whose effect must be captured; it is not permission to erase the parent invocation's policy or UDF restrictions. Disallow unsupported nested operations with typed faults rather than a process hang.

Nested worksheet UDFs get UDF authority even when entered while an event or macro is active. They do not inherit the event Target view or write permission. An event calling an ordinary procedure and an event triggering worksheet recalculation are distinct execution entries.

In a browser, run portable host/VBA operations on their supported owner thread/worker. UI-to-host JS calls remain asynchronous; do not invent blocking cross-worker COM. Native COM/XLL activation is unavailable in pure wasm and must return an explicit platform/capability result. A desktop native bridge, if supplied, is a separate admitted provider connection, not something browser code silently receives.

### 3.9 Macro grants, capabilities, effects and isolation

Keep four independent questions:

| Question | Decision owner | Consequence |
|---|---|---|
| May this workbook's project execute? | Existing macro security and `ProjectGrant` | Off holds on open; explicit per-file enable/unblock is possible. |
| Which host/OS services may it use? | CalcVBA Safe/Full policy plus platform availability | Safe denies the eight gated families; Full permits supported services. |
| Which workbook effects may this invocation perform? | Host entry/effect rules | A macro/event may edit admitted cells; a worksheet UDF remains restricted even under Full. |
| Is this native code trusted in-process? | Application/native provider admission | An in-process native add-in is not contained by a cooperative capability wrapper. |

The current user-enabled/unblocked grant is Full; retain that behavior unless a deliberate product change is requested. Do not relabel it Safe. Conversely, Safe does not inherently forbid ordinary cell edits by an authorized macro: the listed gates concern OS/service families.

Check the relevant constraints at compile/type-library import, binding and dispatch. Retained bindings and objects confer identity, not permanent authorization. Dispatch after a grant change must use current effective policy; queued continuations carry their project/principal and policy lineage explicitly.

Include already-open resources in that rule. The policy comment says file-handle operations pass through unlogged because a denied open leaves no handle (`CalcVBA/crates/calcvba-hal/src/capability.rs:19–22`). An earlier Full open or cross-scope resource transfer is a different case. Require current authority or a deliberately restricted resource lease on subsequent use; inspect/test actual handle scoping before alleging a bypass.

Nested effective capabilities are the intersection of the applicable outer scope, target project grant, current principal, platform and invocation restrictions. Preserve the existing stricter-outer-Safe rule and shared audit. A Full callee cannot widen a Safe caller, and a queued callback must not borrow unrelated ambient TLS.

An exact internally resolved host class can be allowed under Safe. The current exemption expressly excludes arbitrary native fallback and GetObject monikers (`CalcVBA/crates/calcvba-hal/src/capability.rs:571–578`). Returning a foreign object from a host-owned object must not launder foreign member dispatch into this exemption.

For XLLs, registration categories, allowed callbacks and reference argument behavior remain distinct from native trust. OneCalc continues admitting R/U and `#` registrations while never passing a workbook reference, as the owner policy states. Full-host callback legality must be measured per registration/entry category; `#` is not a synonym for unrestricted macro writes.

Audit records follow the originating project/invocation across nested calls, callbacks and async continuations. Keep refusal evidence even when user code catches its visible error. Report overflow/truncation of a bounded audit log rather than claiming a complete audit.

Safe is explicitly a policy gate, not an OS sandbox (`CalcVBA/crates/calcvba-hal/src/capability.rs:12`). A hard stop for uncooperative native code requires a process/isolation boundary. Do not delay the first trusted full-host slice to build speculative containment; state its trust profile truthfully.

### 3.10 JavaScript over the same core

Call the initial surface `DnaExcel`, or clearly mark an `Excel` compatibility mode with its supported members. **The following is proposed DNA behavior, not a claim about Office.js.** Actual Office.js behavior is established by the J experiments.

**Objects and paths.** `RequestContext` owns queued operations, client proxy objects and loaded property snapshots. Object paths are acquisition recipes. A path may refer to an earlier acquisition in the same batch. When first executed, it resolves to a canonical host key. Thereafter the proxy must not repeatedly look up its original name and attach to a newly created same-named sheet.

Application-relative paths such as active worksheet resolve at their first host execution under this proposal; paths with explicit workbook/sheet identity stay explicit. A context is bound to an admitted workbook incarnation, not whichever workbook happens to be active when a network response arrives.

**Load and sync.** `load` queues a read; accessing an unloaded property raises a local typed error. `sync` submits the current queue in order. Reads observe state at their operation's position; returned properties are snapshots, not live host reads. Two JS wrappers may have different loaded snapshots while resolving to the same host key.

**A read/write/calculate example**, initially in manual mode with `A1=1`, `B1 = A1+1` cached as 2, and `C1 = B1` cached as 2:

~~~javascript
// PROPOSED DNA API. Office.js timing is tested in J1.
await DnaExcel.run(async context => {
    const s = context.workbook.worksheets.getItem("S");
    const before = s.getRange("B1");
    const after = s.getRange("C1");

    before.load("values");                         // read cached 2
    s.getRange("A1").values = [[2]];                // authored edit
    context.application.calculate("recalculate"); // calculation barrier
    after.load("values");                          // read calculated 3

    await context.sync();
    // Under this proposal: before.values == [[2]], after.values == [[3]].
});
~~~

The workbook and skin can have published changes before the Promise settles. JS properties are hydrated from their recorded reads when the response arrives; the implementation must not collapse both reads to final state.

**Atomicity and failures.** A batch is an ordered sequence, not an automatic rollback transaction. Each existing host command keeps its own atomic authored-edit boundary. Stop at the first failing operation; do not run the suffix. Preserve earlier effects and any committed effects of the failing operation or its handlers. Return operation ordinals, authored outcome, publication status and fault.

For the initial DNA API, hydrate completed read results before rejecting `sync` and expose the operation receipts in diagnostic error details. Clear the submitted queue, including the skipped suffix, so a later sync cannot accidentally retry it. If Office.js differs, adapt its public compatibility projection; do not change core commit facts to disguise the difference.

Commands execute on the owner lane. Supported synchronous events can run between operations. If a calculation must suspend, its continuation releases locks; required callbacks and permitted host work can proceed. Consequently, a batch is not a snapshot-isolation guarantee across suspension. Recheck object lifetime, authority and calculation eligibility on resumption.

**Await and run.** An ordinary JS await neither flushes the queue nor holds a host transaction. A successful `run` completion performs one final sync if work remains; a rejected callback discards only unsubmitted work. Completed syncs remain committed. Initially reject concurrent sync calls on the same context with a typed error. Confirm these convenience semantics against J4 before advertising Office.js equivalence.

**Tracking.** Untracked remote leases end at run cleanup; already loaded local data may still be read as a snapshot, but further host operations through the expired context fail. `trackedObjects.add/remove` are ordered host operations. Tracking extends object lifetime across admitted context reuse; it does not freeze addresses, preserve permission, prevent workbook close, or revive stale incarnations. Reuse a tracked proxy through an explicitly supported context-reuse API rather than inventing an Office.js overload.

**Cancellation and permissions.** Cancellation before host admission has no workbook effects. Later cancellation is cooperative at safe boundaries, stops the unexecuted suffix and reports committed effects. Cancelling calculation leaves the correct dirty/stale state. A cancellation request cannot promise to preempt an arbitrary native call. Recheck permission after awaits/suspension and before each effect; revocation terminates the remaining authorized work without rolling back prior edits.

Do not automatically replay mutations after a lost connection. Retain the bounded in-flight/completion receipt needed to resolve that request's outcome; otherwise report outcome unknown and resynchronize. This needs request correlation, not a new durable transaction service.

**Custom functions.** Register JS custom functions in the existing callable registry and engine scheduler. Async results carry the workbook incarnation, invocation/callable generation and engine eligibility token. Editing inputs, replacing a project or closing a workbook must prevent an old result from committing as the new invocation's answer. Whether Excel exposes intermediate old results in any captured case is J5; implement observable eligibility accordingly. A custom function cannot acquire macro write privileges by entering a nested `run`.

**Events.** Registration/removal are ordered operations. JS receives asynchronous notifications of the same canonical occurrences delivered synchronously to admitted VBA/COM handlers. Carry source/target identities and event revision; a later read sees its own current query view, not automatically the historical event state. JS handler writes submit later commands. Do not wait for a JS Promise to decide synchronous ByRef cancellation; such a capability is explicitly unavailable in this first JS projection.

**Transport.** Preserve host types and numeric bits in the internal value protocol, including empty/error/missing distinctions and array shape. Use tagged binary64 payloads where ordinary JSON encoding would lose distinctions such as negative zero. Public JS `values` conversion is an adapter contract, measured against Office.js where compatibility is claimed; it must not silently erase the capture evidence.

### 3.11 Interaction table for the five reviews

Each contract below has one named owner. The other reviews supply or consume it; they do not implement competing versions.

| Review interaction | Shared contract and owner | Required agreement |
|---|---|---|
| Object model/JS | Canonical application/object/reference identity and member semantics — **`calc-host::application/automation`** | Other routes use these keys. JS paths and COM pointers are transport carriers, not alternative workbook identities. |
| Graph/recalc | Observation/dependency contract, dirty state, provider eligibility and publication units — **`calc-engine`** | Separate cached observation from dependency registration and forcing. Support suspend/resume without a second evaluator; define cross-workbook calculation ownership. |
| Engine-host-doc | Canonical authored schema and structural edit transform — **`calc-doc-model`** | One transform updates authored references and retained reference descriptors. No adapter-specific insertion or name-resolution rules. |
| Engine-host-doc | Transaction/undo/save-pin timing and operation receipts — **`calc-host`** | Report applied edits even when calculation/events fail. BeforeSave finishes its synchronous decision before pinning the save state. |
| Host-to-skin IR | Semantic delta contents, revision/publication/reset/sequence — **`calc-host::session`** | UI and automation observe compatible state; stale cache and failed publication remain distinguishable. |
| Host-to-skin IR | Wire framing and gap recovery — **`dna-calc-protocol`** | Preserve semantic stamps through worker/WebView2 transport; recover from host snapshots. Rendering may coalesce paint, not semantic events. |
| Concurrency/isolation | Owner-thread admission, callback continuation and reentry categories — **full-host executor in `calc-host`** | Graph workers suspend for VBA; no raw apartment objects cross threads; no lock-held callback. |
| Concurrency/isolation | Native provider admission/lifetime/transport contract — **`calc-host` provider interface** | `calc-xll-host` and `calc-com-host` implement the native transports. Declare trusted in-process versus isolated execution; transport admitted values/opaque keys and never promise policy gates provide memory isolation. |
| Object model/JS ↔ all four others | Event ordering and effective invocation authority — **`calc-host`** | Graph supplies completion facts, doc supplies committed revision, adapters project the event, skins consume consequences, and nested calls retain restrictions. |

The provider-interface row assigns the shared contract to the host; it does not require a new provider-management framework. The application executor remains the sole owner of workbook command admission.



## 4. Staged implementation plan with measurable acceptance

All stages and budgets below are **[PROPOSED]**. This review does not report their execution.

Use the existing test/capture/verdict path. Extend its case record with operation, event and identity observations where necessary; do not create an independent object-model pass counter. Preserve workbook/package hash, input sequence, initial cache, Excel build, adapter/runtime build, selected profile, raw result and refusal lineage.

A compatibility failure remains a failure when the request is in scope. The three cases deliberately assigned to DNA Calc under dc-svqh are explicitly **outside OneCalc's profile**; do not hide them from DNA Calc's full-host acceptance or relabel other refusals as successes.

### Stage 0 — Pin the supported routes and make faults truthful

**Changes.** Record the existing route/profile selection in current acceptance fixtures. Fix the null-EXCEPINFO fault-class split using the existing error path. Expose authored-effect information in the existing command receipt, including the case where the failed operation itself has committed.

**Dependencies and assignment.** Sonnet can implement the local error lowering and receipt plumbing. Opus must settle the meaning of committed/pending/failed outcomes and adapter-visible errors, using E4/E9 where Excel behavior is uncertain.

**Retained behavior.** Keep OneCalc's values-only policy, its exact needs-workbook message, its existing volatility/WorksheetFunction functionality, current shape/materialization refusals and native plugin support.

**Product acceptance.** Invoke Caller/ThisCell with and without diagnostic destinations; a diagnostic pointer must not determine whether the member exists. In a real workbook, induce a provider wait or event error after a write and verify input state, receipt, publication and saved state agree. A refused-before-dispatch edit leaves authored state unchanged.

**Measurements.** Changed verdicts, applied-revision counts, event/callback counts and error carriers. Reuse the relevant existing supported-UDF/native cases; do not claim the reported historical counts have just passed.

**Stop condition.** Every tested failure can be classified as no edit, committed edit with subsequent problem, or an explicitly unresolved transport outcome. Stop before adding member breadth.

### Stage 1 — One real workbook, one full-host route

**Changes.** Add the application registry/root for an actual `HostSession`. Project Application → Workbooks → Workbook → Worksheets → Worksheet → Range metadata and Value2 through existing VBA host tables and dynamic host-object dispatch. Delegate rectangular writes and calculation to today's executor.

**Dependencies and assignment.** Stage 0 and an Opus decision on identity, view and UDF/macro entry rules. Sonnet can implement registry plumbing, known-member descriptors and adapter delegation. Start with one actual workbook; keep the registry shape ready for more.

**Retained behavior.** Keep current calculation behavior and limits, events, project grants, OneCalc routes and unsupported-member refusals. Do not reintroduce the pseudo OneCalc Range.

**Product acceptance.** Open a workbook with A1=1 and B1=A1×10, explicitly calculate/cache 10, then switch manual/automatic in separate cases. VBA early and late host calls write A1=2, read A1/B1, calculate and read again. Compare events, host deltas and save/reopen. A Full worksheet UDF attempting the same write is refused according to the function contract, while an authorized macro can use the supported route.

**Measurements.** One authored transaction per supported rectangle; canonical workbook identity across calls; graph/evaluator instance counts unchanged by adding an adapter; exact type/bit/error observations; command and publication counts.

**Stop condition.** A real file can be edited, calculated, observed by the skin and saved through this route without a shadow workbook. Further members remain explicitly unsupported.

### Stage 2 — Persistent references, names and real collections

**Changes.** Introduce releasable canonical reference handles behind the existing runtime carriers. Add workbook/sheet/name incarnations, scopes, actual collection enumeration and structural-transform integration. Add a second real workbook through the same application registry. Model captured retained-range behavior and preserve event Target identity.

**Dependencies and assignment.** Stages 0–1; E1/E2/E8/E10; shared structural-transform and cross-workbook calculation contracts from the graph/document reviews. Opus decides identity resurrection, name versus returned-range binding and any application-wide scope ambiguity. Sonnet implements the agreed rules and bounded handle lifecycle.

**Retained behavior.** Existing edits and save refusals remain. In particular, do not lift the preserved chart/dialog-reference save refusal reported in `.beads/issues.jsonl:455` just because the new automation reference transform works. Unmodeled retained-reference cases return explicit faults instead of retargeting.

**Product acceptance.** A Range stored in a VBA global survives ordinary calls and permitted edits; rename/reorder does not redirect it; close/reopen and delete/recreate behave exactly as the selected capture. Name scope resolves correctly with conflicting local/global names. Two books expose their actual different contents. Enumerators and multi-area metadata preserve captured order.

**Measurements.** Retain 8,192 small distinct ranges within an explicitly adequate test budget, then release them; verify handle counts return to their warmed baseline. Run a separate create/release loop to distinguish a live-object limit from accumulation. Record transform time versus live references and collection event counts.

**Stop condition.** The supported lifetime/structural cases match captures across real workbooks and no released projection-owned handles accumulate. Do not implement speculative tracking indexes unless these measurements justify them.

### Stage 3 — Native projections and one event authority

**Changes.** Connect the existing native wrappers to canonical full-host keys; replace the single-cell event-target bridge for real workbook events. Generate ABI adapters/type metadata for the admitted member set; preserve legacy plugin facade behavior. Project canonical events to VBA and native sinks; implement the required owner-thread continuation path for callbacks.

**Dependencies and assignment.** Stages 1–2; E4/E5/E6/E9/E10; Opus decisions on the partial Excel-IID facade, event ordering and reentry. Sonnet can generate descriptors, wire wrappers and implement captured cases. Concurrency review supplies the scheduler/continuation boundary.

**Retained behavior.** Keep the OneCalc backend and its reported supported cases. Keep unsupported native facilities unavailable on wasm. Preserve X3/X19 evidence and fixtures; relocate workbook-dependent X19 cases into DNA Calc acceptance.

**Product acceptance.** The same real workbook is seen by portable VBA, PIA/native early binding, IDispatch late binding and XLL callbacks. Test a multi-cell/multi-area Target, cross-workbook identities, synchronous BeforeSave cancel, unsubscribe during callback, handler writes, owner-thread checks and pending-provider calculation. Round-trip the type library and test hidden LCID/result parameters plus ByRef copyback on each supported architecture.

**Measurements.** Exact per-ABI error/result carriers; callback/event order and thread identities; live native/host wrapper counts; callback wait time; zero adapter-created workbook/graph instances.

**Stop condition.** Supported members reach the real host and share one event sequence; every advertised interface has safe known ABI coverage. Do not expand native surface breadth while unproven callback deadlocks or signature gaps remain.

### Stage 4 — JS batches through the same operations

**Changes.** Add RequestContext, proxy paths, load/sync, tracked leases, partial-failure receipts and asynchronous event delivery. Connect the actual browser worker and WebView2/native transport to the existing host gateway. Add a minimal JS custom-function provider through the existing callable registry.

**Dependencies and assignment.** Stage 1 is sufficient to begin basic JS plumbing; persistent tracking depends on Stage 2. Native parity tests depend on Stage 3. Opus chooses the J1–J5 compatibility results and documented DNA behavior; Sonnet implements queue/serialization/proxy code.

**Retained behavior.** Explicitly unavailable native COM/XLL in pure browser mode, all host refusals, UDF effect restrictions and existing skin delta semantics.

**Product acceptance.** Run the mixed read/write/calculate batch from section 3. A later protected-cell failure preserves exactly the observed earlier effects. Test tracking across structural edits, an ordinary await before sync, final run flush, permission revocation during suspension, cancellation before/after a commit and an obsolete async custom-function completion. Compare host state with VBA/native reads where available and with the actual UI.

**Measurements.** One transport submission per explicit sync, operation/commit/event sequence, value payload bytes, live contexts/leases after cleanup, discarded obsolete results and owner-thread responsiveness at cooperative boundaries.

**Stop condition.** The delivered members have a declared DNA contract and a case-by-case Office.js compatibility result. Do not use “Office.js compatible” as an unrestricted product claim.

### Stage 5 — Close the highest-value semantic gaps

**Changes.** Implement captured Value conversion, Formula/Formula2 intent, Evaluate context, further Names/Areas/collection behavior and supported multi-area/shape-mismatch assignment rules. Each additional operation extends the same descriptor/core path. Expand caller shapes and XLL callback categories only from evidence.

**Dependencies and assignment.** Relevant experiments, document preservation and engine interfaces. Opus decides semantic ambiguities and documented divergences; Sonnet implements bounded captured cases. No redesign of numeric kernels or formula syntax is required beyond the entry/context interfaces identified above.

**Retained behavior.** All already delivered routes and explicit unmodeled/platform refusals.

**Product acceptance.** For each newly supported operation, compare portable VBA, native early/late and JS where the surfaces expose equivalent operations. Include xlsx/xlsm/xlsb save/reopen for the admitted document capabilities, using the existing format-specific refusal rules. Verify no adapter “passes” by converting an unknown result to zero, empty or success.

**Measurements.** Per-case gains/losses, exact value/type/error and saved representation, handle/transport cost and relevant corpus regressions. Reuse one shared baseline/final corpus comparison around changes that touch shared semantics; do not rerun the entire bank after every mechanical adapter edit.

**Stop condition.** The selected end-to-end capabilities pass their recorded cases, existing admitted cases have no unexplained regressions, and remaining limitations are explicit. Stop adding framework/generalization work without a concrete next capability.

### Correctness gates versus proposed performance budgets

Correctness is non-negotiable: identity must not retarget, error values must not become exceptions accidentally, forbidden writes must not occur, event/callback order must match the selected captures, and stale async work must not overwrite an ineligible invocation. These are not latency targets.

Initial engineering budgets are hypotheses to measure on named hardware/builds:

| Measurement | Initial proposed budget | Interpretation |
|---|---|---|
| Warm scalar metadata through portable typed adapter | No allocation in the adapter itself; ≤2× direct equivalent core-query time in an isolated microbenchmark | Not a promise that arbitrary Range/string/array operations allocate nothing. |
| JS batch transport | One request/response exchange per explicit sync; no per-cell RPC for a rectangle | Excludes unrelated async events/provider traffic, which is recorded separately. |
| Live handle accounting | Return exactly to the expected retained baseline after cleanup | Measure registry/lease counts, not OS RSS, which may retain allocator arenas. |
| Owner scheduling | Aim for ≤8 ms between cooperative servicing opportunities during host-controlled bulk work | Not a hard deadline or preemption guarantee for arbitrary VBA/native code. |
| Bulk Value2 materialization | Time and bytes proportional to admitted output/edit cells, with checked budget before allocation | Never weaken type/shape semantics to meet the budget; keep refusals visible. |

Do not block the first useful real-workbook slice on these relative performance targets. Investigate measured bottlenecks after the correctness route exists.

## 5. Discriminating experiments against Excel

**[PROPOSED] All expected outcomes below are hypotheses, not measured facts.** Run each branch from an independently reset workbook/process state. A result outside the listed hypotheses is evidence to preserve, not a reason to force the observation into a favored model.

### Common capture protocol

Record full Excel build/channel, platform and Office bitness, locale/LCID, date system, calculation mode, iteration settings, multithreading setting, relevant add-ins/open books and the entry surface. For JS, also record host/runtime kind and available requirement sets. Office.js tests require a real supported Excel JS runtime; native COM/XLL tests require their supported native platform.

Initialize constants and formulas, explicitly calculate to completion, capture the initial caches, then select the requested manual/automatic mode. Record formula entry order as well as calculation command. Use workbook copies or fresh processes when previous dependency discovery, globals or application settings could contaminate a branch.

Capture raw numeric binary64 bits and native VARIANT types where that surface exposes them; also strings, error codes, SAFEARRAY shape/bounds, HRESULT/EXCEPINFO, VBA Err fields, callback return codes, call counts, event depth/order, source identities, thread IDs and publication observations. Keep binary values before formatting or JSON conversion.

For Excel, record public CalculationState and discriminating recalculation/callback counts. **Do not invent a universal public per-cell dirty-bit getter.** Label dirty-state deductions as inferences. DNA Calc can record its internal dirty state alongside the same external observations.

Use an in-memory or external observer log, not worksheet logging from UDFs. Record entry/exit monotonically; wall-clock timestamps alone do not prove order. Check a minimally instrumented control, since logging and observation can affect scheduling.

Save a controlled copy after each decisive outcome, record save-time calculation settings and whether saving itself causes calculation, and reopen independently. A save that recalculates cannot be used as evidence that a previous cache was already fresh. Keep saved-cache observations separate from live answers.

### E1 — Retained Range, deletion, close/reopen and undo

**Input and initial state.** Victim.xlsx has S!A1=10, A2=20, A3=30, manual calculation, no formulas. Retain the reference in an *observer add-in or another workbook's project*, so closing Victim does not destroy the observing globals:

~~~vb
' PROPOSED experiment outline in the observer project
Public held As Object
Sub Acquire(wb As Object)
    Set held = wb.Worksheets("S").Range("A2")
End Sub
' Log each operation separately with its own error capture:
' held.Address, held.Value2, held.Parent.Name, TypeName(held)
~~~

**Entry order.** Acquire; read; insert row 1 in Victim; read again. Fresh branches delete the referenced row, rename/reorder S, delete/recreate S with different constants, or close/reopen the same pathname. Acquire another equal-coordinate reference and compare VBA Is separately from value/address equality; a native observer records controlling IUnknown identity.

**Hypotheses.** After inserting row 1, a structurally tracking reference yields A3/20; a fixed-coordinate reference yields A2/10; an invalidated reference raises a recorded error. After close/reopen, the old object might error, retain limited detached observations, or reattach. The experiment must distinguish these, including per-member differences.

For undo, acquire first, perform a UI structural edit, then UI undo; do not assume a VBA edit leaves an undo record. Compare old-object revival versus continued invalidity versus a distinct new object representing the restored sheet.

**Design consequence.** Sets reference transforms, tombstone behavior, member-specific lifetime rules and undo identity. Any identity revival must be intentional; a new same-named workbook must never be reached merely by repeating a name lookup. Constants remove calculation/cache ambiguity; save/reopen verifies which document was actually edited.

### E2 — Name scopes and a Name versus its returned Range

**Input.** S!A1=10, A2=20, A3=40; T!A1=30. Define workbook N→S!$A$1, sheet-local S!N→S!$A$2 and T!N→T!$A$1. Manual mode; no dirty formulas.

**Entry order.** With S active, resolve N via Application.Range, Worksheet.Range, Evaluate and explicit Names collections; repeat with T active and with the caller/module rooted in S. Retain the local Name and a Range obtained from its RefersToRange. Redefine the Name to S!$A$3; then rename it and reuse its old spelling for a new Name. Record each retained/new object's result and exact error.

**Hypotheses.** Local-over-global worksheet resolution yields 20 on S and 30 on T. Global-only application resolution yields 10 for both. Caller/module-based resolution remains attached to the caller's scope when the active sheet changes. After redefinition, a retained Name can yield the new range/value 40 while an already returned Range remains 20; alternatively both follow to 40, or the old Range invalidates.

**Relative-name branch.** Separately define a relative name using an explicit R1C1 offset of two rows/two columns above an initial C3 base. Use distinct values at A1/B2/C3, move the active cell to D4 and evaluate from E5. Definition-base, active-cell-base and caller-base hypotheses select A1/B2/C3 respectively. Record the exact Name definition and each operation's base rather than relying on implicit UI setup.

**Design consequence.** Decides name-resolution context, stable Name identity, returned-reference binding and what relative-base information the evaluator interface needs. Save/reopen checks scope/definition preservation; it does not establish retained runtime identity across a new open.

### E3 — Cache observation, hidden dependencies and forcing

**Input.**

~~~vb
Function Hidden() As Variant
    Hidden = ThisWorkbook.Worksheets("S").Range("A1").Value2
End Function

Function ArgRead(r As Range) As Variant
    ArgRead = r.Value2
End Function

Function FormulaRead() As Variant
    FormulaRead = ThisWorkbook.Worksheets("S").Range("D1").Value2
End Function
~~~

Add in-memory entry/exit counters without changing worksheet cells. Enter A1=1, B1=Hidden(), C1=ArgRead(A1), D1=A1*2, E1=FormulaRead(), in that order. Run a full calculation and verify the intended initial caches: B1/C1=1, D1/E1=2. Because FormulaRead's hidden-read behavior is itself under test, retain any different full-calculation result as separate evidence; if needed explicitly calculate D1 and then E1 to establish the branch's required cache of 2. Record that seeding order. Switch manual; set A1=2.

**Branch A.** Invoke Application.Calculate, with no unrelated dirty/volatile cells.

- **H1, hidden dependency registered:** B1 changes to 2 and its function count increases.
- **H2, hidden read untracked:** B1 stays cached 1 and its count is unchanged; explicit-argument C1 changes.
- **H3, broader incidental recalculation:** B1 runs despite no dependency. Distinguish by repeated fresh/dirty cases, not final value alone.

**Branch B, fresh setup.** After changing A1, calculate only E1.

- **H1, cached observation:** E1 returns 2 and D1 remains cached 2.
- **H2, demand evaluation:** D1 becomes 4 and E1 returns 4.
- **H3, deferred/retried reader:** the final value may also be 4, but the entry/exit/count trace differs.
- **H4, restricted callback:** a specific error is returned.

**Controls.** Force-all/rebuild, volatile/nonvolatile variants, reversed formula entry and cell placement, multithreading off/on, and matched XLL reference/coercion probes in E10. Read literal and formula sources separately.

**Design consequence.** Determines observation, invalidation and forcing independently. If Excel leaves Hidden untracked, making it dynamically dependent “for correctness” would be an observable divergence and must not be silently adopted. Record live results before any save-time calculation.

### E4 — Change/calculation ordering, nested writes and suppression

**Input.** A1=1, B1=A1+1 cached 2, C1=0. A guarded Worksheet_Change handler logs depth, complete Target, B1.Value2 and entry/exit. When Target is A1 it writes C1=7. Log Worksheet_Calculate, application SheetChange and AfterCalculate through supported VBA/native sinks. No async functions in the first run.

**Entry order.** Fully calculate and clear the log. Set automatic mode, assign A1=2 and log setter return. Repeat fresh in manual mode, then explicitly call Application.Calculate.

**Hypotheses.** Change-before-calculation means the outer handler reads B1=2; calculation-before-Change means it reads 3. Synchronous nested Change yields enter A1 → enter C1 → exit C1 → exit A1; deferred delivery yields enter A1 → exit A1 → enter C1 → exit C1. Capture Calculate/AfterCalculate placement and counts rather than assuming the entire timeline.

**Variants.** Disable EnableEvents, make edit/calculation, reenable and observe. Dropping suppressed events yields no later replay; queuing yields delayed events. In separate branches, explicitly calculate inside the handler, raise an error after writing C1, and reenter a worksheet UDF that attempts a write. Use an explicit recursion guard.

An error-after-write can leave A1=2/C1=7, roll back only nested effects, or roll back the original edit too; inspect both cells and the saved document before concluding which happened. A worksheet UDF entered during the event must be tested as a UDF, not confused with an ordinary procedure called by the handler.

**Design consequence.** Sets event ordering, nested completion, enablement sampling, mutation receipts and invocation authority. Native/VBA/UI observations must describe the same source and Target. Repeat with the async provider in E9 before defining AfterCalculate quiescence.

### E5 — Synchronous cancellation and save pinning

**Input.** An already-saved workbook contains A1=1. Manual calculation and explicit save-time calculation setting. BeforeSave logs, writes A1=2 under a guard and sets Cancel=True. Retain the source workbook identity in the observer.

**Entry order.** Invoke Save; capture ByRef Cancel copyback and method return; inspect in-memory A1; compare disk/package content through a separately opened copy. Fresh branches use Cancel=False and a handler error after the write. Repeat Save As to a new path.

**Hypotheses.** Cancellation suppresses file replacement but retains in-memory A1=2; cancellation also rolls back the handler write to 1; or the save proceeds. Without cancellation, pin-before-handler would save 1 while pin-after-handler saves 2. File metadata alone is insufficient: inspect saved content.

**Design consequence.** Determines save pin timing, effect receipts, source identity through Save As and synchronous error/cancel behavior. A queued JS notification cannot replace the synchronous ByRef decision. Native early/late event sinks must agree on the logical cancel result, with their own correct transport representation.



### E6 — Early/late-bound ABI, defaults, types and error transport

**Input.** One Excel instance and workbook with a scalar double, a 2×2 numeric block, an empty cell, an empty string, a formula error, a date-formatted serial and a currency-formatted double. For example use Value2=45292.25 for the formatted date cell and Value2=123.45678901234567 for the currency cell. Explicitly calculate/cache formulas, then use manual mode.

**Entry order.** Through a PIA/native early-bound client and IDispatch/dynamic client, read Value/Value2 and Address; call Union; vary optional arguments between genuinely omitted, explicit defaults, Empty, Null and an Error variant. Use raw Automation calls where a managed runtime would otherwise conflate those variants. Call an invalid address, then a valid member returning a worksheet error. Subscribe to BeforeSave and copy back Cancel. Test Caller inside the admitted native callback cases and outside them.

**Hypotheses.** Logical values and faults agree across ABI routes, while each has its correct transport representation; or a route differs in omission/coercion/array bounds/copyback. For a date-formatted cell, Value may return Date while Value2 returns Double; an alternative is both Double. For the currency cell, a Currency projection may round to 123.4568 while Value2 retains the source double; an alternative is no Currency conversion. These are predictions to record, not results asserted here.

Repeat direct Invoke with and without EXCEPINFO for a failing Excel member. Separately run the DNA OneCalc needs-workbook fixture with both diagnostic shapes; Excel does not have DNA's custom values-only profile or error text.

**Controls.** x86/x64 where supported, LCIDs and supported builds; capture member kind, DISPID, selected vtable slot, full wire signature, HRESULT/Err/IErrorInfo and raw result. Do not infer the wire signature merely from a successful dynamic call.

**Design consequence.** Determines generated ABI lowering and shared semantic conversion/error rules. Any crash is a fixture failure to diagnose, not an acceptable unsupported-member response. Preserve the historical LCID discovery without claiming it is currently unfixed.

### E7 — Formula/Formula2 and WorksheetFunction/Evaluate

**Input.** A1:A3 contains 1,2,3. E2:E4 and G2:G4 are clear, manual mode, no obstructing merged/spill cells. In separate fresh branches set E2.Formula="=A1:A3" and G2.Formula2="=A1:A3"; then run the same explicit calculation command.

**Hypotheses.** A legacy implicit-intersection setter yields E2=2 while the array-intent setter yields G2:G4=1,2,3. Alternatively both normalize to the same scalar/array semantics, or Formula2 is unavailable on that build. Capture both formula getters, displayed and raw values, spill extent, error, authored text and saved/reopened behavior. Repeat with a deliberate spill obstruction to distinguish entry validation from calculation failure.

For function-error transport, compare Evaluate("MATCH(99,A1:A3,0)") with WorksheetFunction.Match(99, Range("A1:A3"), 0), under the same sheet/name context.

- **H1:** Evaluate returns a successful error value, while WorksheetFunction raises a host/VBA exception.
- **H2:** both return an error value.
- **H3:** both raise a method error.

Use an in-memory error-catching wrapper so logging does not consume or replace the result.

**Design consequence.** Requires distinct formula-entry intent and per-member error projection over the same evaluator. A missing Formula2 member is a build/platform capability observation, not evidence of a numeric defect. Save/reopen also tests whether the document layer preserves the chosen semantics.

### E8 — Calculation scope across real workbooks

**Input.** Open A and B. Each has S!B1=Counted(A1), where Counted returns its numeric input and increments a per-book in-memory counter. A!A1=1, B!A1=2. Enter formulas in A then B; fully calculate to cache 1/2; clear counters; switch manual; edit inputs to 10/20.

**Entry order.** From independent initial states, invoke A!B1.Calculate, A's Worksheet.Calculate, Application.Calculate, Application.CalculateFull and Application.CalculateFullRebuild. Repeat with B initially clean to distinguish scope from force-all selection.

**Hypotheses.** Local scope updates A to 10 while B stays cached 2; application scope updates both to 10/20. A dirty-only application calculation need not rerun a clean B, while a force-all operation would increment its counter. Capture counts, events, raw values and command-return order; if Excel does more work, retain that trace.

**Linked branch.** Add A!C1 as a link to B!B1, explicitly seed its cache and repeat. Cross-workbook dependency demand may update B even for a selected reader; that is why the unlinked branch is required before concluding that a command has global scope.

**Design consequence.** Sets application versus workbook coordination and cross-workbook dependency requirements. It directly tests whether a local Book1/workbook mapping has leaked into the full Application API. Save/reopen both files to separate actual live computation from persisted external-link caches.

### E9 — A write while calculation waits, fails or is cancelled

**Input.** A1=1; B1=Gate(A1), implemented by a controlled asynchronous native provider on a supported Excel build. Complete the initial request so B1 is cached 10. A subsequent request for input 2 is held behind a barrier released by an independent provider worker. Use no synchronous UDFs initially, then add a VBA dependent in a separate branch.

**Entry order.** Set automatic mode; log before assigning A1=2; assign it from a macro and from external automation in separate runs; log return/error, cached B1 and CalculationState. Release the barrier to produce 20. Repeat in manual mode with an explicit calculation command.

**Hypotheses.**

- **H1, synchronous completion:** the setter/calculation returns only after release, with B1=20.
- **H2, admitted pending calculation:** it returns earlier, A1=2 and B1 remains cached 10 until completion.
- **H3, pending represented by a worksheet state/error:** it returns earlier with a distinct typed B1 result, then 20.
- **H4, failure after commit:** the caller receives an error while A1 remains 2; the calculation/cache status is separately observable.

Record Change, Calculate and AfterCalculate before/after release; “AfterCalculate fires on setter return” and “AfterCalculate waits for the pending work” make different traces.

**Failure/cancel branches.** Returning a worksheet CVErr is a *successful calculated error value*, not a simulated engine infrastructure failure. Test that separately. Use user cancellation where supported while the provider is held; inspect whether A1 stays 2, which cache remains and what later calculation does. An independent watchdog records a timeout but must not call a killed run successful.

**Design consequence.** Determines setter waiting, operation receipts, callback servicing and application quiescence. The VBA-dependent branch detects an owner thread waiting for a worker that needs that same owner. Save during a wait is a separate operation with its own possible wait/cancel semantics; record it rather than assuming it preserves the earlier cache.

### E10 — XLL entry categories, Caller and actual macro commands

**Input.** On a disposable workbook, A1=1, B1=0. Register matched native functions under each admitted category: ordinary UDF, macro-sheet-equivalent, and thread-safe where supported. Each logs entry/exit/thread and attempts only the tested callback: caller/reference inspection, coercion of A1, or a controlled write to B1. A separate registered command performs the same write. No arbitrary cross-thread raw COM pointer use: use the platform's admitted callback or marshaled automation mechanism.

**Entry order.** Fully calculate baseline; manual mode; calculate the single function cell. Separately invoke the command through Application.Run. Separately request an admitted queued macro and observe before UDF return, after return, and at command entry. Repeat under automatic mode and the stale formula-source setup from E3.

**Hypotheses.** A worksheet entry rejects the write and B1 stays 0; an allowed special category performs it synchronously and B1 becomes 7 with immediate event effects; or an admitted queue leaves B1=0 during the UDF and changes it to 7 at the later command boundary. An ordinary synchronous nested Run might preserve the UDF restriction or constitute a distinct allowed entry; capture rather than infer from the method name. Thread-safe callbacks may be refused instead of marshaled.

**Caller branches.** Use single-cell, CSE-array and dynamic-spill entry cases on builds supporting them, plus outside-call observation and a supplied Caller Index. Competing outcomes include the top-left cell, the whole array/spill reference, another typed caller value, or a specific error/refusal. Capture type, area shape and identity; equal strings alone are insufficient.

**Design consequence.** Sets function-category effects, callback view, caller shape and the real edit-aware macro entry. OneCalc retains the owner-directed values-only refusal; these reference/caller cases belong to DNA Calc. Do not turn native trust, a Full grant or `#` registration into universal worksheet-write permission.

### E11 — Multi-area geometry, assignment and collection iteration

**Input.** Manual mode with constants and no dirty formulas. Form Union(B2:C3, F5, C3:D4), then reverse argument order; add the explicit F5-first permutation Union(F5, B2:C3, C3:D4). Record Areas, each address, Count, Rows.Count, Columns.Count, Cells/Item defaults and enumeration. All areas are on one explicit sheet.

**Hypotheses.** Area multiplicity gives Count=9; counting unique cells gives 8; a bounding-rectangle implementation gives 20 and loses the holes. First-area Rows.Count changes from 2 to 1 when F5 becomes the first area; another normalization policy may reorder the areas. Capture actual area lists and iteration order rather than inferring them from Count.

**Assignment branches, each fresh.** Start disjoint B2:C3 and F5:G6 at zero. Assign a scalar, then an explicit 1×1 array, then an explicit 2×2 array through the disjoint range and a single rectangle separately. Hypotheses include applying to all areas, applying only to the first area, rejecting before mutation, or modifying a prefix/placing worksheet errors for unmatched elements. Record every target value/type/error and event Target; an exception alone does not prove no effects.

**Collection branch.** Use worksheets S,T,U; enumerate and rename/remove/add a sheet at a known iteration point in a disposable workbook. A snapshot cursor retains its original identity sequence (possibly with a stale-object error); a live cursor can skip/include entries based on current positions; a fail-fast cursor errors at mutation. Record object identities as well as names.

**Design consequence.** Decides AreaSet ordering/multiplicity, assignment atomicity and cursor semantics. Preserve the current explicit multi-area/shape-mismatch refusal until these rules are modeled. Save/reopen confirms authored effects but is not evidence about the runtime iterator.

### J1 — Office.js load/write/calculate ordering

**Runtime and input.** Use actual Office.js in a supported Excel runtime. Initialize the section 3 example through an independent observer: manual mode, A1=1, B1=A1+1 cached 2, C1=B1 cached 2. Confirm available calculation API/enum for that runtime before executing the equivalent operation; an unavailable API is recorded as capability absence.

**Batch outline.**

~~~javascript
// EXPERIMENT outline in actual Office.js; use this runtime's calculation enum.
await Excel.run(async context => {
    const s = context.workbook.worksheets.getItem("S");
    const before = s.getRange("B1");
    const after = s.getRange("C1");
    before.load("values");
    s.getRange("A1").values = [[2]];
    context.application.calculate(/* recalculate enum */);
    after.load("values");
    await context.sync();
    record(before.values, after.values);
});
~~~

**Hypotheses.** Reads at queued positions yield before=2/after=3. Loads consolidated against final state yield 3/3. A calculation command that does not provide the assumed completion boundary can leave after=2 at sync, with a later change. Inspect unloaded property access before sync separately.

**Design consequence.** Determines whether the proposed ordinal query semantics match Office.js. Capture event order and values through the independent observer as well. These are API observations; they do not establish Excel's internal locking, message queue or scheduler architecture.

### J2 — Office.js partial failure and effects of the failing operation

**Input.** Protected sheet with A1/C1 unlocked, B1 locked, all three zero. Establish protection before JS begins. Manual mode, no formulas/events in the first branch.

**Entry order.** In one batch set A1=11, B1=22, C1=33; call sync. Catch errors during local enqueue separately from sync rejection. Read all three through a fresh successful context or native observer after the error.

**Hypotheses.** All-or-nothing/no-effect yields 0/0/0. Stop at the failing operation yields 11/0/0. Continue after the error yields 11/0/33. A final 0/0/0 alone cannot distinguish host preflight from rollback; event logs and observable operation admission help distinguish them.

**Variants.** Queue a load before the failing write and inspect whether its result hydrates on rejection. In another workbook, have the Change handler perform a second write and then raise an error after an otherwise admitted edit. First record whether that handler error actually rejects sync: if Excel contains or reports it independently, this is an event-error observation. If sync rejects, it tests whether the *failing operation itself* has committed effects.

**Design consequence.** Sets failure ordering, partial results and the Office.js error projection. Preserve committed-state facts in the core even if the public JS API exposes less detail. Save/reopen records the surviving authored changes.

### J3 — Tracked paths, structural edits and stale objects

**Input.** S!A1=10, A2=20, A3=30, manual mode. JS acquires A2, loads address/values, adds it to tracked objects and syncs. Use the actual runtime's supported context-reuse form.

**Entry order.** After the first batch, insert row 1 through an independent UI/automation route; use the retained proxy in a later admitted context. Fresh branches use an untracked proxy, rename/delete/recreate S, and close/reopen the workbook.

**Hypotheses.** A tracking reference yields A3/20; a fixed-coordinate reference yields A2/10; a path-invalidated proxy reports a recorded stale/InvalidObjectPath-style error. Tracking may extend lifetime without making every path structurally stable, so record both properties independently.

**Design consequence.** Sets tracked lease behavior, path invalidation and structural reference requirements. This experiment does not assume a particular unverified Excel.run overload. JS wrapper equality is not proof of host identity, and loaded snapshots can remain readable even when further remote operations are invalid.

Closing a workbook may tear down its owning JS runtime. Record runtime termination as such; do not label inability to resume as an observed stale-object error. Exercise old proxies only where the runtime survives, and use an independent observer for reopen behavior otherwise.

### J4 — Ordinary awaits, active-sheet resolution and run completion

**Input.** Two sheets S/T, each A1=1; S active; manual mode. During Excel.run, acquire an active-worksheet proxy and queue A1=2, then await a controllable external barrier before sync. At the barrier inspect both sheets from another route and switch active sheet to T.

**Hypotheses.** Eager acquisition binds S, so the eventual write lands on S. First-host-execution resolution binds T, so it lands on T. If work is buffered, both cells remain 1 before sync; eager dispatch changes one already.

**Separate branch.** Queue a write and return successfully from run without explicit sync: implicit final flush yields 2; no flush or an error leaves 1. In another branch throw before any sync: unsent work is discarded under one hypothesis, but any eager effects survive under another. A prior explicitly successful sync is a separate committed boundary.

**Design consequence.** Sets convenience semantics and prevents a misleading “await holds a workbook snapshot” model. DNA grant revocation during the barrier belongs to the local policy controls below; do not assume Excel exposes the same revocation API.

### J5 — Async custom functions, stale completion and JS notifications

**Input.** On an Excel JS custom-functions runtime supporting the required asynchronous fixture, Gate(x) waits on a controlled service and returns 10×x. Seed B1=Gate(A1) with A1=0 and completed cache 0. Use automatic calculation for the main branch.

**Entry order.** Set A1=1 and hold its request; set A1=2 and hold the replacement request. Release input-1's result (10), then input-2's result (20). Fresh branch reverses completion order. Log function invocation/cancellation identities and JS changed/calculated notifications; add a VBA/native event observer only where that host combination is supported.

**Hypotheses.** Obsolete results are rejected, so the final result is 20 regardless of release order. Old results may appear transiently before being superseded; record that publication. A last-completing-result rule can leave stale 10 when input-1 completes last. Captured cancellations may distinguish early abort from late result rejection, but only if the chosen runtime actually exposes cancellation.

**Controls.** Manual mode plus explicit calculation; close/reopen or replace the function provider while a request is pending; save/reopen after a documented quiescent point. Record any JS runtime teardown separately from cancellation/stale-object errors. If the runtime disappears, use the independent observer and controlled service to detect whether a late response affects the reopened workbook. Missing runtime facilities are explicit capability results.

**Design consequence.** Determines observable eligibility fences, async event/completion timing and stale-result behavior over the one graph. No conclusion about Excel's internal thread model follows merely from Promise/event order.

### DNA-specific policy and isolation controls

These are **[PROPOSED] product acceptance experiments, not claims about an equivalent Excel Safe/Full facility**:

- **Grant mapping:** open the same project under Off, Safe, Unguarded and explicit per-file enable/unblock. Verify open/event entry and the current no-policy/Safe/Full mapping. With a supported workbook macro write and a worksheet UDF write, demonstrate that OS capability policy and spreadsheet entry effects are separate.
- **Compile/bind/dispatch:** use a harmless fixture that requests native type-library loading, external activation and a member on an already-held external object. Exercise Safe→Full and Full→Safe nested calls; retain the exact policy/audit parent. A denied outer capability must remain denied after cached binding or a deferred callback.
- **Previously opened resources:** open a temporary file under Full, then attempt Get/Print#/EOF through the retained file number under a Safe nested/downgraded context where the runtime allows that handle to be reached. Hypotheses are inaccessible handle, denied use under current authority, or inherited authority allowing access. The packet's policy comment leaves handle operations unlogged on the premise that a denied open creates no handle (`CalcVBA/crates/calcvba-hal/src/capability.rs:19–22`); it does not prove what happens to a handle opened earlier under Full.
- **Host-class exemption:** exact DnaCalc host class, unknown class and GetObject moniker under Safe. The host object may be supplied internally, while arbitrary activation remains denied. Then attempt a prohibited UDF write through that safe object; object ownership must not bypass effect checks.
- **Queued JS/native work:** queue a write, revoke the grant before execution or during a controlled await, then resume. Rejected-before-execution leaves the target unchanged; a prior committed edit survives while the suffix is refused. Audit must retain the original principal, not whichever project happens to occupy TLS.
- **Isolation boundary:** a trusted native fixture intentionally stalls under a controlled timeout. In-process cooperative cancellation may be unable to stop it; an isolated process can be terminated but leaves already acknowledged host edits intact. This tests the stated boundary, not a promise that Safe contains arbitrary native code.

Use tiny temporary/disposable inputs and the existing audit/verdict harness. These controls close specific authority gaps; they are not a request for a separate security management framework.

## 6. What to stop or delete

1. **Stop growing the removed OneCalc pseudo workbook.** Keep dc-svqh's values-only policy and its supported function/plugin services. Keep R/U and `#` registrations admitted as directed, without passing references. Keep volatile behavior and WorksheetFunction. Caller/ThisCell and workbook-dependent VBA Range requirements retain their explicit profile refusals.

2. **Stop using the local Book1 shell as evidence of a full application.** For DNA Calc, replace its semantic backend with real host keys and commands. Retain the narrow OneCalc compatibility shell only where existing supported plugins require it; do not delete that route before replacement acceptance.

3. **Delete duplicate semantic routes as each adapter migrates.** Eliminate independent range parsing, name resolution, cell-value stores, worksheet writes, calculation dispatch and structural tracking from projections. A carrier table, COM wrapper or JS property cache can remain; none should own spreadsheet truth.

4. **Retire the 4,096-entry interning table as the full host's lifetime policy.** Replace it with releasable canonical handles and explicit resource budgets. Do not “fix” it by making all objects immortal or removing all bounds. Preserve an intentional typed refusal until the replacement is tested.

5. **Stop guessing native interfaces.** A member name, DISPID and successful dynamic call do not specify a dual vtable. Remove guessed-signature stubs and handwritten duplicate metadata as generated, ABI-checked replacements land. Keep own activation identities and label the narrow Excel-IID facade honestly.

6. **Delete error translations that confuse unknown member, unsupported capability, calculated error value and failed operation.** Repair the null-EXCEPINFO split. Do not turn an edit that committed before a later failure into “nothing happened,” and do not retry it automatically.

7. **Stop treating every callback read as demand evaluation or every read as a new graph edge.** Use measured read/entry semantics. Do not “improve” Excel parity by making undeclared dependencies more responsive unless the divergence is explicitly chosen and recorded.

8. **Stop using Full, native trust or a host-owned object as worksheet-write authorization.** Also stop treating Safe as OS containment or assuming an earlier Full resource grant becomes harmless merely because future opens are denied. Preserve entry-mode restrictions, nested policy and audit lineage.

9. **Replace independent event enablement and event synthesis in full-host adapters.** Keep one canonical source, full Target identity and measured completion order. Do not manufacture AfterCalculate on every local return or suppress nested events just to avoid reentry work.

10. **Defer an expansive Office.js clone, universal descriptor compiler, persistent distributed command log and speculative handle index.** Deliver one real workbook, real read/write/calculate, persistent references, native projection and a small JS batch route. Expand only for captured capabilities. Do not advertise WebGPU or browser-native COM merely because the boundaries could support them.

11. **Preserve compatibility evidence.** Keep X3/X19 fixtures, the historical hidden-LCID failure, Range/Union/Caller measurements, the owner-policy removal record, old reviews with their evidence labels, current document-save refusals, and all capture/build/provenance fields. History is useful input; it is not current executable coverage.

**Recommended next implementation unit:** one actual DNA Calc workbook exposed through the existing VBA host adapter, with stable workbook/sheet/reference handles, Value2 read/write, one calculation route, truthful operation receipts and the existing skin/save path. That is concrete capability. Native and JS projections should then reuse it, preserving the same authored state, graph, identities and verdict lineage.

