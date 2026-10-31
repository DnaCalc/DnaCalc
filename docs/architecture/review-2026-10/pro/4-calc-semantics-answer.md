# Excel calculation semantics: evidence, experiments, and a contract for DNA Calc

**Answer to `4-calc-semantics-prompt.md` · 31 October 2026**

**Target:** the supplied Excel 16 / Microsoft 365 build 20527, 64-bit Windows on AMD. This is a public-source review and an executable experiment specification, **not a report of newly executed Excel tests**. Appendix C is treated as supplied experimental evidence; its underlying workbooks, raw traces and hashes were not independently inspected.

## Recommendation

Keep visible-first computation, but make **permission to expose a computed value** a separate decision from permission to compute it. There is no general basis for treating every cached read as a request for fresh calculation, every finished command as globally fresh, or every function evaluation as a single invocation per tick.

The first implementation should preserve four independent things:

1. The calculation command's scope and forcing rules.
2. The values, metadata and temporary scenario state visible to each observer.
3. Semantic evaluation and invocation identities, including legitimate repeat evaluations.
4. The distinction between a computed result, its publication, and its freshness provenance.

Two refinements to Appendix B are particularly important. Native XLL callbacks can be required to unwind and be called again; preserving an activation is therefore not a universal callback rule. Also, whole-SCC and whole-data-table atomic publication are proposed implementation strategies, not established descriptions of every Excel observation boundary. Their outward behavior must be measured. [MS18] [MS19]

The ten priority experiments are **E01–E10**. The remaining catalogue, **E11–E30**, closes the other calculation-sensitive gaps and maps the performance-only items in Appendix B to the appropriate DNA Calc benchmarks.

---

## 1. How to read the evidence

| Label | Meaning | Appropriate use |
|---|---|---|
| **D** | Explicit Microsoft product, API or public file-format documentation. | A documented expectation, with its stated scope and version limitations. |
| **D-legacy** | Microsoft documentation whose applicability or discussion is tied to older Excel versions. | A strong test hypothesis; exact modern traces still need confirmation. |
| **R** | A named practitioner's original published observations. | Well-established public knowledge, with provenance; not a vendor guarantee. |
| **O** | The target-build observations supplied in Appendix C. | Retain as bounded regression evidence for those fixtures and that environment. |
| **I** | Deduction or proposed DNA Calc design. | An argument whose premises must be stated. |
| **U** | Undocumented, ambiguous, conflicting, or unmeasured for the target. | An experiment and an explicit compatibility boundary. |

“Documented” does not mean that every explanatory article supplies a complete operational specification. The Microsoft recalculation overview, individual method pages and performance guides differ in precision and occasionally in wording. Prefer a specific API contract for its interface, preserve disagreements, and let a controlled target-build observation settle an implementation decision.

### What Appendix C already establishes

The following are **O**, not results of this review:

- Clean ordinary recalculation and unrelated edits did not advance a nonvolatile VBA UDF's state.
- Reassigning an input to the same value did advance it. A recalculated upstream formula also caused its dependent UDF to run when the upstream result was unchanged.
- Explicit range, full and rebuild calculations invoked nonvolatile UDFs.
- Hidden VBA reads did not establish the same invalidation behavior as explicit formula arguments.
- The observed volatility requests were local to the formula cell: last executed explicit request won; an ordinary call did not clear an earlier request; untaken calls made no request; nested helper requests affected the caller; reevaluation without a request could remove volatility.
- Initial evaluation sometimes called a function twice. This is evidence against a fixed single-call assumption, not a universal two-call rule.

The supplied account also says the successful run used safe mode after an unrelated add-in crash, and that an observation-only expectation prevented an overall certification pass. Preserve that qualification when importing these results into the test catalogue. Keep the original observation files; do not convert their prose into broader scheduling guarantees.

### What “exact” must mean

**I:** Exactness needs an observable contract, not just a final cell matrix. Include cell values and errors, callback return values, invocation multiplicity, permitted callback order, event-visible caches, spill geometry, saved caches, and the response to cancellation and external returns.

For random, clock and external inputs, distinguish identical outputs under identical admitted inputs from merely similar distributions. A finite black-box corpus cannot establish every possible hidden calculation order. If a target Excel trace is itself variable, record the variation and the environment that caused it; do not silently declare arbitrary DNA Calc traces equivalent.

---

## 2. Semantics compendium

### 2.1 Dirty marking, dependencies and the calculation chain

**D:** Edits mark direct and indirect dependents for recalculation. Excel maintains a calculation sequence and can revise it when a required formula is not ready. [MS01]

**D:** Dependency maintenance occurs even in Manual mode. A formula can be evaluated repeatedly during one recalculation, and dependents can still calculate after an upstream result remains numerically unchanged. [MS02]

**D:** The saved calculation-chain part records an order of formula cells, not the dependency graph. It neither dictates runtime order nor provides a complete invocation trace. An application can reconstruct its own chain. [MS03]

**I:** Consequently:

- A value version and an invalidation version represent different facts.
- An unchanged value cannot, by itself, justify suppressing an effectful dependent.
- A topological order is sufficient only for a restricted class of computations whose permitted observations are independent of that order.
- Reading `calcChain.xml` can help explain a fixture's history, but cannot reveal internal passes, thread interleavings, or every UDF invocation.

**U → E02, E03, E04, E15:** Exact dirty propagation after narrow calculation, dynamic-reference discovery, formula reentry, and the influence of entry history, saved chains and threading.

### 2.2 Modes and explicit calculation commands

**D:** Desktop Excel's calculation mode is shared by the open workbooks in an application instance. The first opened document normally supplies the initial mode; subsequent documents inherit the current mode. Saving records the effective mode. [MS04]

| Mode | Documented broad behavior | Consequence for DNA Calc |
|---|---|---|
| Automatic | Recalculate affected work after changes. | An edit admits work; viewport interest does not define its entire extent. |
| Automatic Except Data Tables / `xlCalculationSemiautomatic` | Automatic calculation with What-If data-table work excluded from ordinary automatic updates. | Deferred tables remain distinct pending work. “Tables” here does not mean ordinary ListObjects. |
| Manual | Ordinary automatic recalculation is deferred until requested. | Edits and old formula caches can coexist. Explicit evaluation and formula-entry actions still need separate rules. |

The enumeration specifies these three modes. Newer UI wording can differ; record the actual enumeration and visible label rather than deriving semantics from a label alone. [MS05]

The table below describes the intended command categories, **not an unconditional promise about unavailable external inputs, disabled sheets, cancellations or asynchronous operations**.

| Operation | Scope and forcing | Freshness limit / measurement needed |
|---|---|---|
| `Application.Calculate`; F9 outside formula editing | Ordinary recalculation across open workbooks. | Does not mean every clean formula is forcibly reevaluated, or that closed external sources are refreshed. |
| `Worksheet.Calculate`; Shift+F9 | Calculation of the specified/active worksheet. | Do not assume dirty precedents on another sheet are first brought current. |
| `Range.Calculate` | A selected range; modern performance guidance says dependencies **within that range** are resolved. | Out-of-range prerequisite treatment and clean-cell forcing must be pinned by mode. |
| `Range.CalculateRowMajorOrder` | Cell-by-cell in row-major order. [MS63] | A separate operation; it must not inherit ordinary dependency scheduling. |
| `Application.CalculateFull`; Ctrl+Alt+F9 | Force calculation across all open workbooks. | Stronger than ordinary dirty calculation; not a promise to fetch current external reality. |
| `Application.CalculateFullRebuild`; Ctrl+Shift+Alt+F9 | Rebuild dependencies and force full calculation across open workbooks. | Rebuild can alter future scheduling history. |
| `Range.Dirty` | Mark the range for calculation; automatic mode can cause recalculation. | It is a mutation, not a dirty-state getter. |
| `Range.Value` / `Value2` | Retrieve a value. | No documented unconditional calculate-and-drain contract. Test the callback/read context. |
| Save | Serialize according to calculation and save policy. | The pre-save calculation option matters; inspect actual file caches. |

The application/sheet/range APIs establish the targets; the full and rebuild methods establish their stronger work. Keyboard mappings come from Microsoft's current calculation support article. [MS06] [MS07] [MS08] [MS09] [MS10] [MS11]

**Documentation ambiguity:** the older overview gives different `Range.Calculate` behavior in Manual and automatic modes; the newer performance guide describes range-local dependency resolution. Neither justifies one universal rule for every mode. **E02 must test both dirty and clean targets.** [MS01] [MS02]

**D:** `EnableCalculation=False` prevents worksheet recalculation; changing it back to true triggers calculation. `ForceFullCalculation` changes subsequent calculation behavior and is not a synonym for the one-shot `CalculateFullRebuild` method. Its documentation says forced mode persists until Excel restarts. [MS12] [MS13]

**U → E02, E09, E26:** Which scoped commands clear which dirty status; treatment of disabled sheets; selected multiple sheets and multi-area ranges; dependencies crossing workbooks; setting changes; tables and pending external work.

### 2.3 VBA observations, arguments and effects

**D:** VBA UDFs have macro-sheet-equivalent read permissions, including access to uncalculated cells. Ordinary worksheet XLL callbacks have different permissions. Reading an uncalculated cell is therefore not automatically a request to calculate it. [MS17]

**D-legacy:** Microsoft's custom-function account describes early calls with Null/Empty arguments and later calls with current values. It also explains that dependencies hidden inside a UDF are not inferred from its argument list. Worksheet UDFs are restricted from ordinary workbook/environment mutations. [MS16]

**O:** Appendix C supplies the stronger, target-specific hidden-read and state-counter findings described in §1.

**I:** A worksheet UDF can still be effectful even where worksheet writes are prohibited: advancing module state, reading a clock or external service, and inspecting other cells' caches are observable. “Nonvolatile” concerns scheduling; it does not establish purity.

Distinguish at least:

| Read path | Required distinction |
|---|---|
| Formula argument `F(B1)` | Binding/dependency, argument representation, coercion and permitted provisional invocation. |
| VBA `Range.Value2` inside a zero-argument UDF | A callback observation at a particular visible cache frontier. |
| VBA argument declared `As Range` | Live reference versus eager value conversion. |
| VBA argument declared `As Double` | Coercion can occur before the function body; log the actual received value. |
| A stored Range from an earlier call | Object identity/lifetime and current observation semantics are independent of original argument binding. |
| Macro/COM value read outside calculation | A distinct host operation; not automatically a worksheet dependency or a calculation command. |

**D:** `Value2` avoids the Date and Currency subtypes used by `Value`; the Range default member also has `Value` forwarding behavior. This affects a typed callback bridge even when the underlying numeric cache is unchanged. [MS14] [MS15]

**U → E01, E04, E21:** Own-caller reads, dirty explicit arguments under each signature, hidden dynamic references, saved objects, same-value assignment routes, and exact repeat-call schedules.

### 2.4 Volatile functions and propagation

**D:** Volatile formula cells and their dependents are recalculated in applicable recalculation work. The overview names NOW, TODAY, RANDBETWEEN, OFFSET and INDIRECT, with argument-dependent CELL, INFO and SUMIF. That list is not an exhaustive function registry. [MS01]

| Function or mechanism | Evidence and operational distinction |
|---|---|
| `NOW`, `TODAY` | Volatile clock/date observations. Volatility is not a periodic timer subscription. |
| `RAND` | A new random value is generated on recalculation; unchanged arguments do not imply reuse. [MS21] |
| `RANDARRAY` | Produces an array of random values. Fixed output dimensions and volatile output values are separate concerns. [MS22] |
| `RANDBETWEEN` | A random-valued volatile expression; also relevant to unstable spill sizing. |
| `OFFSET`, `INDIRECT` | Volatility and reference resolution are distinct. Even a currently stable referent does not justify removing the documented volatile policy. |
| `CELL`, `INFO` | Parameter-dependent behavior and ambient/metadata inputs need a function-and-argument matrix. |
| `Application.Volatile` | The Boolean request defaults to true; outside a worksheet UDF it has no scheduling effect. [MS20] |
| XLL registration | Registration flags and reference argument types can establish volatility independently of VBA's runtime request mechanism. [MS18] |

**I:** Separate (a) whether an action starts calculation, (b) which formulas that command admits, and (c) which observations an admitted evaluation makes. A volatile result may remain equal, and a nonvolatile formula may still be forcibly reevaluated.

Do not implement a universal “all edits create one new random value per cell” rule. Repeated calls, array elements, name evaluations, circular sweeps, data-table scenarios and spill-sizing passes may be distinct semantic occurrences.

**U → E05, E12, E20:** Untaken built-in volatile branches versus untaken VBA calls; exact CELL/INFO/SUMIF cases; clock sampling granularity; cross-workbook propagation; volatility changes after conditional execution; whether an unchanged volatile result still advances each dependent under a particular command.

### 2.5 Dynamic arrays: shape is calculation state

**D:** A dynamic array has an anchor and a spill area; obstructions prevent successful placement. Spilled children do not behave as independently editable formula cells. Dynamic arrays linked between workbooks have limited support, requiring both workbooks open for supported refresh. [MS23]

**D:** Microsoft explicitly describes additional resizing passes and indeterminate-size `#SPILL!`, using `=SEQUENCE(RANDBETWEEN(1,1000))` as an example. Obstruction, grid-edge, merged-cell and table cases have their own constraints. The documentation does not specify a pass cap or a simultaneous-anchor winner. [MS24]

**I:** Preserve shape, value, dependencies and footprint coherently. A previously empty coordinate requires an absence/occupancy certificate if a pending producer might claim it. A value-only backward graph cannot prove that such a blank stays blank.

For the supplied growth example, a fully current interpretation after `Z1=10` gives `A10=10` and `C10=11`; after shrinking to one row it gives `C10=1`. Those are arithmetic consequences of the proposed formulas, **not predictions about a narrow Excel calculation command**.

**U → E05, E06:** Sizing attempts, conflict order, reuse/removal of old footprints, invalidation of readers when spill children disappear, child-only range calculation, error precedence, and effects of unfinished anchors.

### 2.6 Formula, Formula2, implicit intersection and serialization

**D:** `Formula` uses implicit-intersection evaluation; `Formula2` uses array evaluation. Excel can translate between their textual representations by exposing or suppressing `@`. Identical input text through the two APIs can therefore have different semantics. [MS25]

**D:** `@` preserves a scalar; for a range it selects by the formula's row/column intersection; for an array value it takes the top-left element. Reference-valued and array-valued intermediates are consequently different. [MS26]

**D:** `SavedAsArray` reports legacy-array serialization behavior and can be Null for a mixed range. It is not a general test of present spill occupancy. [MS27]

**I:** Formula interning/coalescing needs dialect, binding context, reference/value types and save representation. Retaining only normalized formula text loses observable information.

**U → E11:** Formula entry, paste/fill, legacy CSE entry, save/reopen and transformation routes; scalar formulas with array serialization; explicit versus inserted `@`.

### 2.7 LET, LAMBDA, names and expression evaluation

**D:** LET gives a local name to a computed result and supports reuse without recomputing that binding expression within the evaluation. The documentation does not specify every unused-binding or ordering case. [MS28]

**D:** LAMBDA supports up to 253 parameters. Excessive recursion can produce `#NUM!`; the documentation does not promise a fixed maximum recursive depth. A naked, uncalled LAMBDA in a cell gives `#CALC!`, and wrong argument count gives `#VALUE!`. [MS29]

**D:** Defined names can be evaluated in each consuming formula; unused names are not evaluated just because full calculation was requested. A name is not necessarily a single globally memoized value. [MS02]

**R:** Charles Williams's original probes distinguish scalar IF/IFNA/CHOOSE short-circuiting from IFS/SWITCH evaluation. His array-selector probes show different branch-evaluation behavior. Treat the reported traces as evidence about those formulas and tested versions, not a complete language specification. [R01] [R02]

**I:** There are several independent reuse boundaries: a LET binding, a LAMBDA invocation, an expression occurrence, an array-helper element, a defined-name use, a cell evaluation and a calculation pass. They must not collapse into one `(tick, cell)` cache.

Use `LET(draw,RAND(),draw-draw)` in the probe. A descriptive name avoids the name-syntax ambiguity of the prompt's single-letter `r`; Excel names have restrictions related to reference syntax. [MS28]

**U → E05, E13, E14, E16:** Evaluation order, laziness, helper traversal, reference-preserving parameters, closures, name scope, recursive depth by call shape, and error/side-effect ordering.

### 2.8 Circular calculation and iteration

**D:** `Application.Iteration` enables circular calculation; `MaxIterations` limits iteration count; `MaxChange` controls the change threshold. These API descriptions do not define the complete convergence norm or sweep algorithm. [MS30] [MS31] [MS32]

**D-legacy:** Microsoft's performance guide describes preliminary circular discovery, single-threaded sheet-by-sheet iteration, and participation of circular dependents and volatile work. It does not establish an exact number of discovery invocations or alphabetical sheet ordering. [MS33]

**R, conflicting detail:** Decision Models reports alphabetical sheet-name order and row-major order during iteration, with independent volatiles excluded from each sweep. That exclusion is narrower than Microsoft's wording. The account contains substantial legacy-version discussion. **Neither exact participant rule should be promoted to an untested build-20527 invariant.** [R03]

**I:** An SCC identifies a graph component. It does not select seeds, discovery calls, within-sweep visibility, participating downstream formulas, treatment of nonnumeric values, stopping rules, or how scoped calculation enters the cycle.

Keep circular diagnostics separate from a cell's cached result. Do not invent a universal Excel `#CIRC!` worksheet value. The warning, retained value, later reevaluation and iteration-disabled behavior belong in the observation record.

**U → E07, E17:** Initial caches; one-iteration meaning; strict versus non-strict threshold; aggregate versus per-cell stopping; continued calculation after a limit; independent SCC interaction; dynamic cycle membership; errors; branches; sheet order; and downstream callback timing.

### 2.9 What-If data tables and TABLE()

**D:** Excel's What-If table evaluates substituted input combinations and stores the scenario results. It performs ordinary calculation before scenario recalculation; table work is single-threaded. [MS02]

**D:** `Range.Table(RowInput, ColumnInput)` creates a data table using single-cell input references. Use that API to build fixtures rather than treating displayed `TABLE(...)` text as an ordinary user-enterable function. [MS34]

**D-legacy:** Excel tolerates a result calculation referring back into its data table without the usual circular-dependency error. [MS35]

The overview calls table work asynchronous relative to regular recalculation. That wording alone does not settle when a particular COM Calculate method returns. [MS01]

**I:** Use a scenario-specific input view and a matching observation/result view. A VBA callback inside the scenario may need to see substituted inputs. Separately determine what unrelated observers can see, what base caches remain afterwards, and whether cancelled runs expose partial table output.

“Restore the base inputs” is not a sufficient specification: formula caches, name evaluation, volatile occurrences, table feedback and callback reads also participate. Conversely, an assumption that all scenarios can be deduplicated or run independently is unsafe before testing duplicate inputs and table-induced cycles.

**U → E08:** Scenario traversal, repeated inputs, initial/final baseline calculations, name behavior, callback observations, partial publication, cancellation restoration, feedback values and Automatic Except Tables.

### 2.10 Multithreaded calculation and XLL registration

**D-legacy:** Excel can run eligible independent cells on workers. VBA, COM/XLM functions, non-thread-safe XLL work and unsafe built-ins use the main thread. Documented unsafe examples include INDIRECT, CELL with format/address, PHONETIC, GETPIVOTDATA, CUBE functions, ERROR.TYPE, HYPERLINK and some context-dependent cases. A single unsafe branch does not make every independent formula worker-ineligible. [MS36]

**D:** XLL `$` permits thread-safe registration; `#` grants macro-sheet-equivalent permissions and cannot be combined with `$`; `!` requests volatility. Macro-sheet-equivalent functions with R/U reference parameters default to volatile. R/U can carry references; P/Q carry values/arrays. [MS18]

**D:** An ordinary XLL encountering an uncalculated reference can receive `xlretUncalced`, unwind, and be invoked again after the precedent is calculated. After that status or `xlretAbort`, only the permitted cleanup call `xlFree` may precede return. Keep the API status separate from a successful API call returning an Excel error value. [MS18] [MS19]

**U:** Microsoft pages disagree about an exact failure code for some unsafe callbacks. Record the raw code rather than asserting all failures are `xlretNotThreadSafe`. [MS18] [MS36]

**I:** Thread safety is not purity. An XLL using an atomic counter can be concurrency-safe yet expose order through its returned values. Parallel eligibility, determinism, observer access and safe speculative replay need separate capability fields.

**U → E04, E18:** Native retry traces, dynamic reference discovery, lasting invalidation edges, worker eligibility, code/status distinctions and observable parallel variation.

### 2.11 Asynchronous XLLs and RTD

**D:** An asynchronous XLL call has an opaque handle; retained argument data must be deep-copied before its entry point returns, including pointed-to strings/arrays. The permitted off-calculation-thread completion callback is `xlAsyncReturn`. Its success/failure and returned worksheet data are distinct observations. [MS37] [MS38]

**D-legacy:** Excel retains formula state around the async call, completes it on return, and calculates dependents in later work. [MS01]

**I:** A completion must match a particular call, not merely a cell/provider generation. Preserve the already executed formula prefix unless a new semantic evaluation is actually required. An async return, an RTD update and a refreshed external link are different event types.

**D:** RTD uses a COM topic lifecycle. UpdateNotify announces that data is available; RefreshData retrieves topic/value updates. The RTD function documentation describes continuing updates under Automatic calculation. The documented throttle default is 2,000 ms, and a changed throttle persists. [MS39] [MS40] [MS41] [MS42] [MS43]

**I:** A subscription can produce many updates, and notification bursts may not correspond one-for-one to visible cell revisions. Give a DNA Calc subscription its own generation and admitted update sequence without claiming those exact identifiers are exposed by Excel.

**U → E10, E19:** Superseded handles, duplicate completions, edits during a suspended call, cancellation, Manual-mode RTD, equal-value updates, coalescing, close/reopen and event timing.

### 2.12 External links

**D:** Opening a workbook has an explicit link-update option; `UpdateLink` is a separate operation. [MS44] [MS45]

**D:** Microsoft documents recalculation using a destination workbook's stored external-link table, including cases where updating links was declined. That external cache can differ from the destination's saved formula-cell caches. [MS46]

**I:** Track at least three versions: the source's available state, the admitted link snapshot, and the formula result produced from that snapshot. A full calculation can be correct relative to the admitted snapshot while that snapshot is old relative to the external source.

**U → E22:** Open versus closed source, same versus another process, dirty source caches, missing sources, update rejection, external names, unsupported function paths and dynamic-array links.

### 2.13 Opening, saving and file caches

**D:** In Manual mode, `CalculateBeforeSave` controls requested pre-save calculation; the property survives calculation-mode changes. [MS47]

**D:** SpreadsheetML `<f>` stores a formula and `<v>` can store its last-calculated value. Workbook `calcPr` records settings/flags such as `calcId`, `calcMode`, `calcOnSave`, `fullCalcOnLoad`, `forceFullCalc`, `calcCompleted`, `fullPrecision` and iteration controls. [MS48] [MS49]

**D:** Microsoft's current support guidance describes version-related full recalculation on opening, subject to mode exceptions. [MS11]

**I:** A saved cache is evidence of a previously calculated result, not proof of current workbook-wide freshness. A successful save is not proof that pending external work, excluded tables, disabled sheets or event-induced edits have all been freshly evaluated.

Opening combines file flags, the application's current settings, version handling, link policy and open-event code. Do not state “all volatiles always recalculate on open” without those qualifications. `fullCalcOnLoad` here is a file-format flag, not a reason to invent a VBA property.

**U → E09, E23:** Precedence of settings, exact save calculation scope, Save versus SaveAs versus SaveCopyAs, event ordering, cancelled calculation, and async/table completion.

### 2.14 Precision as displayed, dates and other semantic context

**D:** `PrecisionAsDisplayed` changes workbook calculation to use displayed precision. Microsoft's support guidance states that the rounding can permanently change stored values; increasing precision afterwards does not reconstruct removed digits. [MS50] [MS11]

**I:** Number formats therefore cannot be universally excluded from semantic invalidation. Do not infer that every width/font/zoom change changes arithmetic: General-format and action-specific effects need measurement.

**D:** `Date1904` is a workbook property. Corresponding ordinary dates in the two systems differ by 1,462 serial days. Excel preserves its fictitious 1900 leap day for compatibility. [MS51] [MS52] [MS53]

**I:** Date context, numeric policy and coercion routes belong in validity certificates. Raw Value2 assignment, Date-valued COM assignment, clipboard copy, sheet copy and external links need not share the same conversion policy.

**D:** CELL can observe width, number format and other metadata; an omitted reference can use the selected cell at calculation time. INFO("origin") observes scrolling position, and INFO("recalc") observes calculation mode. [MS54] [MS55]

**I:** Distinguish a UI action changing an observable input from that action triggering immediate calculation. Scheduling interest must not accidentally mutate the Excel-visible selection or window state.

**U → E20, E24, E25:** Formatting and view triggers, precision rounding boundaries, date toggles, epoch-sensitive function results and conversion routes.

### 2.15 Events, completion and cancellation

**D:** Worksheet.Change excludes value changes produced by recalculation. Worksheet.Calculate is a later calculation event; AfterCalculate follows completed pending refresh/calculation work and the transition to `xlDone`. [MS56] [MS57] [MS58]

**D:** `CalculateUntilAsyncQueriesDone` is documented for pending OLEDB/OLAP queries. It is not a generic API for finishing every possible asynchronous provider. [MS59]

**D-legacy:** XLL CalculationEnded/CalculationCanceled lifecycle events are explicitly documented as absent for programmatic recalculation. UI-triggered and COM-triggered runs therefore need separate event tests. Native lengthy functions can cooperate with user interruption through `xlAbort`. [MS60] [MS61]

**I:** Event handlers are observers too. A progressive publisher must reproduce event-visible cache boundaries and distinguish ordinary result commits from edits. User cancellation, a scheduler time slice, superseding a pure calculation and shutting down a provider require different transitions.

**U → E10, E19, E27, E28:** Method return versus events, reentrant edits, partial caches, later restarts and the lifetime of outstanding operations.

---

## 3. Amendments to the proposed recalculation contract

Everything in this section is **I: a proposed implementation contract**, grounded in the evidence above and the supplied counterexamples. These are not names or structures claimed to exist inside Excel.

### 3.1 Separate admission, observation and publication

```rust
struct ReadRequest {
    purpose: ReadPurpose,          // invalidation, callback, replay, diagnostic
    policy: ReadPolicy,
    view: ObservationViewId,
}

enum ReadPolicy {
    FormulaDependencyWithinCommand,
    CachedObjectModelRead,
    XllWorksheetCallback,          // may require xlretUncalced + unwind
    XllMacroEquivalentCallback,
    ExplicitFreshSnapshot,         // separate DNA Calc operation
}

struct ReadCertificate {
    reads: Vec<ObservedVersion>,
    bindings: BindingVersions,
    occupancy: OccupancyCertificate,
    environment: EnvironmentVersions,
}

struct ObservedVersion {
    location: Location,
    value_version: ValueVersion,
    invalidation_version: InvalidationVersion,
    source: ReadSource,            // current result, cached, file, scenario, link
}

struct ResultStamp {
    command: CalcCommandId,
    evaluation: EvaluationId,
    completion: CommandCompletion,
    freshness: Freshness,
    reads: ReadCertificate,
}
```

The certificate must describe intentionally cached reads without making them fail validation merely because they were stale. Validation asks whether the invocation saw a **permitted** value in its observation view. A stronger freshness claim separately asks whether those inputs represented the desired revision.

This avoids the main flaw in “Settled within demand”: a completed C1-only calculation that used old B1 can be command-complete while remaining stale relative to the workbook's latest edit. Do not silently freshen B1 to simplify the certificate.

### 3.2 Track evaluations and callbacks separately

```rust
struct EvaluationId {
    command: CalcCommandId,
    phase: CalcPhaseId,            // ordinary, sizing, iteration, scenario...
    locus: EvaluationLocus,        // cell/name/array/helper/other
    occurrence: u64,
}

struct InvocationId {
    evaluation: EvaluationId,
    call_site: CallSiteId,
    occurrence_path: OccurrencePath, // recursion/helper elements/nested calls
}

enum EvaluationProgress {
    Yielded(Continuation),
    WaitingForAsync(AsyncHandle),
    UnwoundForDependency(ReinvokeRecord),
    Completed(ResultUnit),
    Cancelled(CancellationRecord),
}
```

The governing rule should be:

> Preserve effects already performed by a suspended activation. Do not manufacture a second invocation as a scheduler retry. When the compatibility protocol requires a new semantic evaluation or callback entry, allocate a new identity and preserve its effects.

This handles both resumable OxVba execution and the native XLL unwind/reinvoke path. They must not be conflated.

Random input identity should include the semantic evaluation and occurrence, with array-element or helper-path identity where relevant. A physical retry can reuse recorded inputs; a genuine new spill-size evaluation must remain able to draw again. This is a replay design, **not evidence that a selected generator reproduces Excel's hidden random stream**.

### 3.3 Preserve observable cache transitions

A conservative fence around an unrestricted callback must cover possible **cache publications it can observe**, not just other callbacks. In the absence of a read-set bound, that can mean workbook-wide visibility control. Include other open workbooks if the object model permits observing them.

Later, separate speculative/private results from an Excel-compatible logical cache. A callback reads the logical view; physical workers can finish additional pure cells without exposing them prematurely.

The UI also needs a declared policy. If “visible-first” displays a speculative result that differs from Excel's intermediate display, that is an intentional presentation behavior to specify. It must not leak into the callback-compatible cache or silently become a freshness claim. A presentation layer with extra intermediate states is not automatically exact trace equivalence.

### 3.4 Give special calculations explicit sessions

- **Spill session:** candidate shape, attempted footprint, old footprint, obstruction versions, conflict arbitration and semantic sizing history.
- **Iteration session:** observed seeds, discovery work, ordered participants, per-sweep state, convergence state and downstream invocation history.
- **Table session:** scenario inputs, reference/name context, callback view, scenario outputs, base restoration and cancellation/publication policy.
- **External operation:** async call completion, RTD subscription update or external-link snapshot admission.

These sessions can yield internally. Whether an entire session is externally atomic is a compatibility question; do not infer it from the desirability of transactions.

### 3.5 Make completion and save policy explicit

```rust
enum ExternalOperation {
    AsyncReturn { call: InvocationId, generation: ProviderGeneration, value: Value },
    RtdUpdate { subscription: SubscriptionId, sequence: u64, value: Value },
    LinkSnapshot { source: ExternalSourceId, revision: SourceRevision },
}

struct CompletionReceipt {
    command: CalcCommandId,
    scope: CalculationScope,
    status: CompletedOrCancelled,
    result_view: ObservationViewId,
    admitted_external: ExternalFrontier,
    remaining: PendingSummary,
}

enum SavePolicy {
    ExcelCompatible { calculate_before_save: bool },
    FreshSnapshot { external_policy: ExternalSnapshotPolicy },
}
```

A successful compatible save pins one model revision and one result view, possibly containing permitted old caches. A fresh snapshot export can be stronger, but must be a separate requested operation. A continuous RTD stream has no permanent “all future updates completed” state.

### 3.6 Keep function capabilities orthogonal

At minimum, distinguish deterministic output, state effects, cache/metadata reads, dependency protocol, volatility, thread eligibility, reentrancy, async lifetime and formula evaluation mode.

For optimization, “no UDFs in this cone” is not a complete criterion. Built-ins can observe environment, time, random inputs, reference shape and precision settings. Conversely, a declared and verified pure UDF could eventually be eligible for safe reuse. Keep the default conservative and promote individual capabilities with evidence.

---

## 4. Common experimental protocol

### 4.1 Capture the initial state, not just the workbook formulas

For each case, preserve:

- Original workbook bytes and hashes; workbook/sheet names and ordering; formula-entry sequence; Formula/Formula2 dialect; defined-name definitions and scope.
- Excel Version, Build and CalculationVersion; OS, bitness, CPU model/vendor; locale, separators and date/time context; MTC settings; installed/loaded add-ins and the safe-mode versus normal-mode lane.
- Application calculation mode, iteration controls, CalculateBeforeSave, EnableEvents and relevant workbook flags, PrecisionAsDisplayed and Date1904.
- The complete operation sequence, command boundaries, typed values/errors, available spill metadata, invocation and event traces, and saved package bytes.

Warm-chain, newly built, reopened and rebuild runs are distinct initial conditions. Use fresh process/workbook clones for comparisons. For open-order tests, do not accidentally open a controller workbook first and thereby choose the mode you are trying to measure.

### 4.2 Keep the logger from becoming the experiment

Preallocate an in-memory log. Capture an observation once, store that captured value, and return it. Flush only after the tested command. Avoid worksheet writes, Debug.Print timing as a primary clock, extra Evaluate calls and DoEvents inside UDFs.

A VBA logging wrapper makes a previously native formula depend on VBA execution. It can change threading and order. Pair every such fixture with a plain-formula control. For MTC tests use native thread-safe logging buffers, not VBA or a global logger lock that serializes the test.

Record both API status and returned value/error for C API calls. After `xlretUncalced` or `xlretAbort`, use only native-memory logging and the allowed cleanup path; do not make extra diagnostic callbacks.

Do not claim direct access to a per-cell dirty bit. `Range.Dirty` changes state, and `CalculationState` reports application state. Record controlled edits and subsequent invocations; label the dirty-state interpretation as inferred. [MS10] [MS62]

### 4.3 Separate repeat counts from invisible internal passes

A counter proves that the counter UDF was entered. It does not count all hidden built-in random draws, identify every native spill-sizing attempt, or prove the scheduling of an uninstrumented formula.

Use deterministic witness functions to distinguish candidate algorithms, and retain uninstrumented RAND/spill cases as separate tests. Do not replace RAND with VBA Rnd and assume the resulting stream or scheduling is equivalent.

### 4.4 Inspect saves without recalculating the evidence

Read `.xlsx`/`.xlsm` ZIP parts directly before any reopen. Retain worksheet `<f>`, `<v>`, cell types, array/table attributes, workbook and worksheet calculation properties, external-link parts, relevant extension metadata and `calcChain.xml` if present.

Do not round-trip the evidence through a library that silently rewrites formulas, flags or extension parts. If testing XLSB, decode the relevant records with a reader based on the public format specification; an XLSB-to-XLSX save through Excel is a separate transformation, not an untouched cache observation.

Compare typed live values and raw serialized tokens. For doubles, retain binary64 bits in the controller as well as decimal text; capture VBA Variant types before COM marshalling can erase a Date/Currency/Error distinction.

### 4.5 Replication and acceptance

Start with approximately ten fresh repetitions per priority condition; increase repetitions only where order or timing varies. This is a proposed starting point, not a statistical guarantee. Keep setup calls separate from measured calls, and use a watchdog for a deliberately nonconverging, blocked or long-running case.

Each probe record should contain:

```text
case_id, fixture_hash, platform_profile, setup_operations,
accepted_initial_observations, tested_operation, live_observations,
callback_trace, event_trace, saved_artifact_hashes,
outcome_class, competing_explanations, design_decision
```

An unexpected setup value means **invalid trial**, not a new semantic result. A busy/rejected COM call is an observation about allowable interleaving; do not force reentry to make a desired test succeed. Native XLL and RTD cases require small local test providers, driven by the COM/VBA supervisor.

---

## 5. The ten most design-critical experiments

### E01 ★ — Dirty VBA reads, macro reads and own-caller observations

**Setup.** Sheet S: A1=1; B1=`=A1+1`; C1=`=PeekB1()`. PeekB1 reads `ThisWorkbook.Worksheets("S").Range("B1").Value2` once, logs the captured value and caller, then returns it. In Manual, explicitly calculate B1 and then C1; verify B1=2/C1=2. A full calculation alone does not establish this seed for a hidden observer with no explicit dependency.

**Operations.** Switch to Manual, set A1=10, and record B1 through both ordinary COM and a macro read. Calculate C1 alone; calculate B1 alone; calculate C1 again. Use separate clones for sheet, Application.Calculate, Full and Rebuild. Add a UDF reading its own caller and test both an existing cache and a newly entered formula.

**Record.** Every B1/C1 value, Variant type/error, callback observation, entry/exit order and command boundary. Use an instrumented-producer variant only as a secondary test.

**Outcome → design.**

- C1 observes 2 before B1 calculation and 11 afterwards: hidden observation must preserve cached values; eager freshening is wrong.
- It observes 11 before explicit B1 calculation: first establish whether the command calculated B1 or setup already did so; freshening cannot be inferred from the value alone.
- Behavior varies by command/context: encode the actual read policy in the invocation context.
- Self-read returns an old value, Empty or an error: preserve that outcome; do not turn every self-read into a synchronous dependency cycle.

### E02 ★ — Command scope, forcing and post-command dirtiness

**Setup.** A1=1; B1=`=A1*2`; C1=`=B1+1`, initially B1=2/C1=3. Add a clean nonvolatile counter formula and an unrelated dirty branch. Create same-sheet, cross-sheet and cross-workbook variants.

**Operations.** In Manual set A1=5. On fresh clones calculate C1 only, B1:C1, B1 only followed by C1, the containing sheet, Application.Calculate, Full and Rebuild. Repeat in all three modes, logging calculations caused by both mode changes and input assignments. Capture values immediately before the target command: an automatic-mode trial whose setup already repaired the chain tests clean-target forcing, not dirty out-of-scope reads. Mark the dirty-scope variant unavailable unless its stale precondition actually holds. For row-major comparison reverse the dependency direction, e.g. A1=`=B1+1`, B1=`=C1*2`, C1 a changed input.

After each narrow command, run ordinary F9 **without a new edit**, then Full. Add Union ranges in different area orders; a selected-sheet group; a disabled sheet; and ForceFullCalculation as separate factors.

**Record.** Values after every step; clean/dirty counter calls; what later F9 repairs; command return and events; all effective flags.

**Outcome → design.**

- C1 becomes 3 while B1 remains 2: narrow completion can legitimately consume an old precedent.
- C1 becomes 11 and B1 becomes 10: the tested command reached B1; determine whether that is the general rule or a special condition.
- F9 after the narrow command does or does not repair C1: this fixes clearing/re-dirtying behavior, not merely the first command's scope.
- Clean nonvolatile counters run on range but not ordinary calculation: forcing is a separate admission dimension.
- Row-major and dependency-aware calculations differ: preserve two commands.

This probe is also the direct resolution of the conflicting Range.Calculate documentation.

### E03 ★ — Cache-publication order and visible-first scheduling

**Setup.** A long pure chain ending at B1 and an independent VBA formula PeekB1 with no explicit edge to B1. Establish distinguishable old/new B1 values. Include a second observer reading several independent chain outputs.

**Operations.** Dirty the inputs and invoke the same command with different formula-entry histories, cold/warm/rebuild chains, active sheets, producer positions, viewport positions and MTC settings. Keep actual viewport/selection state fixed in a separate lane where only DNA Calc's internal interest changes.

In DNA Calc, deliberately prioritize the observed producer before or after the observer while keeping explicit dependencies valid. Compare against the relevant Excel observation records.

**Record.** Values seen by each callback and event observer, final pure values, effective environment and invocation order. Do not wrap every pure cell in VBA just to recover its finish order.

**Outcome → design.**

- Equal final pure values but changed callback results: publication is part of the effect boundary.
- Excel observations depend on history/environment: retain those inputs in the compatibility profile.
- DNA interest changes an observation that is stable in matching Excel trials: fence publication or supply the old logical read view.
- No difference in these trials: only the tested class is supported; this does not prove arbitrary cache observers commute.

### E04 ★ — Semantic reinvocation versus accidental retry

**Setup.** Use a stateful VBA counter with explicit/hidden late precedents, and an ordinary XLL UDF accepting an R/U reference. The XLL records an entry counter before attempting `xlCoerce`; it obeys the required unwind on `xlretUncalced`. Build ordinary, thread-safe and macro-equivalent registration variants separately.

The macro-equivalent R/U variant defaults to volatile. Record that condition and add a separate variant requesting `xlfVolatile(false)` before comparing ordinary/warm call counts; otherwise a volatility difference can be mistaken for a retry difference.

Also compare VBA parameters `As Range`, `As Variant` and `As Double`, with `F(B1)` versus `F(B1+0)`. Include an unused explicit argument.

**Operations.** Cold entry, first calculation, repeated warm calculation, Full, Rebuild, same-value input reassignment and a changed upstream formula whose output stays equal. Choose a dynamic reference inside the native callback in another variant so the engine cannot necessarily pre-order it from a simple argument edge.

**Record.** Entry counters, arguments/types, callback status, native thread, immediate unwind, subsequent reentry, final results and producer observations.

**Outcome → design.**

- Two native entries surrounding `xlretUncalced`: a genuine reinvocation must remain possible, including its repeated prefix effects.
- Provisional VBA arguments followed by current ones: preserve those distinct semantic calls.
- Same-value edits or equal upstream results advance the counter: value-only memoization or effectful early cutoff is invalid.
- A single call in one arrangement: the attempted late-dependency path may not have been reached; it does not establish a one-call invariant.

### E05 ★ — Random occurrence, LET reuse and volatile spill passes

**Setup.** Place these formulas in separate nonoverlapping areas:

```excel
=RAND()-RAND()
=LET(draw,RAND(),draw-draw)
=LAMBDA(x,x-x)(RAND())
=DrawName-DrawName
=LET(fn,LAMBDA(RAND()),fn()-fn())
=MAP(SEQUENCE(4),LAMBDA(x,RAND()))
=LET(draw,RAND(),MAP(SEQUENCE(4),LAMBDA(x,draw)))
=RANDARRAY(2,3)
=SEQUENCE(RANDBETWEEN(1,1000))
```

Define workbook name DrawName as `=RAND()`; add two separate consuming cells. For deterministic witnesses, replace random calls with a sequence-returning UDF in a separate workbook.

For spill sizing, use the anchor formula `=SEQUENCE(SizeUdf())`, with SizeUdf returning a controlled sequence: constant 2; 2,3,3,3; alternating 2,3; 2,2,3; and an error on a selected call. Arm/reset the sequence after formula-entry and setup evaluations; retain those setup calls in a separate trace. Test volatile and nonvolatile variants.

**Operations.** Separate entry-time evaluations from clean ordinary, forced range, full and rebuild commands. Repeat relevant cases inside iteration and tables.

**Record.** Full returned arrays, deterministic witness traces, final footprint/error, downstream invocations, and calculation boundaries.

**Outcome → design.**

- Different repeated-call values refute one draw per cell/tick.
- LET shares a value within an evaluation while separate lambda calls do not: preserve those boundaries.
- A changing-size expression errors while a frozen draw succeeds: freezing across semantic size evaluations changes semantics.
- Controlled dimension prefixes distinguish candidate stabilization algorithms; do not infer the native RAND draw count from the witness UDF.
- A zero difference between two random values alone is inconclusive; use repeated trials and deterministic witnesses.

### E06 ★ — Blank occupancy, spill growth/shrink and simultaneous conflicts

**Setup A.** Z1=1; A1=`=SEQUENCE(Z1)`; C10=`=A10+1`. Fully calculate.

**Operations A.** Set Z1=10 in Manual. Compare C10-only, anchor-only, both together, sheet and full commands, then shrink back to 1. Test a command containing a spill child but not its anchor.

**Setup B.** Z1=0; A3=`=IF(Z1,SEQUENCE(4,3),1)`; B1 has the same formula; F1=`=SUM(B3:C4)`. With Z1=1 the proposed spills overlap without either anchor being inside the other's footprint.

**Operations B.** Vary entry order, dirty order, individual anchor calculations, cold/warm/rebuild history and MTC. Add a literal obstruction, `=""`, formatted blank, merged cell, ListObject region and grid-edge case on separate clones.

**Record.** Anchor values/errors, full footprints, child values, reader values, available spill-parent metadata, clearing of old children and saved array metadata.

**Outcome → design.**

- Growth changes a formerly blank coordinate: freshness requires absence/producer evidence.
- Shrink leaves an incorrect old child or reader after a command required to complete that scope: removal must invalidate occupancy and consumers. An old out-of-scope reader after a narrow command can be intentional.
- Conflict winners vary with operation order/history: reservation order is semantic input.
- Both anchors fail: do not invent a first-finisher winner.
- Child-only calculation expands scope or returns old values: implement that command-specific rule.

**Scale extension.** In DNA Calc, place geometrically increasing numbers of unresolved anchors above/left of visible blanks, with most eventually scalar. Record blocker counts, inspected candidates, first provisional time and **first settled time**. Excel can supply value observations and timings; it cannot expose DNA Calc's may-spill-index counters.

### E07 ★ — Iteration seeds, one-sweep meaning and participant order

**Setup.** Switch D1=0:

```excel
A1 = IF($D$1,(B1+1)/2,0)
B1 = IF($D$1,(A1+1)/3,0)
```

Enable iteration and establish **observed** zero caches before setting D1=1 in Manual. The false branch is not assumed to remove every static cycle. If this seeding route fails, use a disposable OOXML fixture with explicit zero caches, open in verified Manual/no-update mode, and reject any trial whose initial cache changed.

**Operations.** Fresh clones for MaxIterations=1, 2 and a larger value, with a small positive MaxChange. Compare ordinary/full/rebuild/range/sheet commands; reverse cell positions; split the pair across sheets with opposed alphabetical and tab orders; change names without moving tabs, then tabs without renaming.

**Candidate algorithm signatures from zero, not asserted Excel results:**

| Candidate single sweep | A afterwards | B afterwards |
|---|---:|---:|
| A first, immediately visible to B | 1/2 | 1/2 |
| B first, immediately visible to A | 2/3 | 1/3 |
| Simultaneous/Jacobi | 1/2 | 1/3 |

Add a downstream PairLog(A1,B1), an independent volatile counter, a volatile counter depending on A1, dependents of each, an unrelated nonvolatile counter and a second independent SCC. Record `Application.Iteration` from the logger without modifying it.

**Record.** Seeds, all observed pairs, invocation sequence, participant counts, settings, warnings, events and final caches.

**Outcome → design.**

- Candidate signatures distinguish within-sweep visibility; extra calls require accounting for discovery or additional evaluations.
- Downstream callbacks before final convergence refute “private SCC then DAG once.”
- Independent volatiles run once per command versus per sweep: settle the documentation/report disagreement.
- Name order, tab order or history changes results: retain the corresponding semantic ordering.
- Independent SCCs interact in stopping/participants: independent private convergence needs refinement.

### E08 ★ — Data-table scenarios, observations, feedback and restoration

**Setup.** A1=10, A2=2, B1=`=A1*100+A2`. Put `=$B$1` in E1, scenarios 1,1,2 in D2:D4, then create:

```vb
ws.Range("D1:E4").Table ColumnInput:=ws.Range("A1")
```

For a two-input variant, D1 points to B1, E1:F1 hold A1 scenarios and D2:D4 hold A2 scenarios:

```vb
ws.Range("D1:F4").Table RowInput:=ws.Range("A1"), _
                              ColumnInput:=ws.Range("A2")
```

For the one-input table, add result headers using an input-reference name, a reference-valued INDEX name, RAND, a counter, a hidden-read VBA UDF peeking A1, and an explicit-argument counterpart **before** creation, expanding the table range to include all result columns. A two-input table has one corner result formula; test alternative outputs using separate complete two-input tables.

**Operations.** All modes; ordinary/full/range calculation; repeated F9; duplicate/permuted scenarios; unrelated edit. On a clone with established caches, set B1 to `=A1+N(E2)` and recalculate twice with ordinary iteration disabled. Finally run a long table, attempt supported observations during it, and cancel at several points.

**Record.** Scenario-observed inputs and caches, per-call name/callback values, table output, base A1/A2/B1 before and after, event/return timing, API busy responses, partial outputs and post-cancel state.

**Outcome → design.**

- Duplicate scenario inputs produce distinct effects: no value-tuple deduplication.
- Hidden reads see substituted inputs: callback views must join the scenario context.
- Table feedback reads previous or newly produced outputs: preserve that specific traversal/result view.
- Base inputs restored but B1 retains a scenario cache: restoring only inputs is insufficient; reproduce the observed post-run cache policy.
- Partial output is externally observable: whole-table atomic publication requires an observation layer or a narrower compatibility claim.
- COM cannot interleave: do not manufacture reentrancy with DoEvents merely to expose private state.

### E09 ★ — Save truth table and open-time cache handling

**Setup.** Use E02's chain, fully calculated, then Manual A1=5 with old B1/C1 caches. Add separate variants containing a volatile formula, stale table, disabled sheet and pending async call.

**Operations.** Independent clones for CalculateBeforeSave false/true; Save, SaveAs and SaveCopyAs; Manual and Automatic Except Tables. Baseline with events off, then logging handlers, then a guarded BeforeSave handler that edits one precedent.

Log the mode transition separately and verify caches immediately before Save. Switching out of Manual can already recalculate the arithmetic chain; use the deferred What-If table as the intentionally stale Automatic Except Tables fixture.

Before reopening, inspect the saved ZIP. Reopen separate copies under controlled Manual and Automatic conditions, first workbook versus after a controller. Add synthetic file-flag variants for `calcId`, `fullCalcOnLoad`, `calcCompleted` and worksheet `sheetCalcPr`. Keep genuine old-version files separate from synthetic flag tests.

**Record.** Pre-save/live/post-save values, raw caches/flags, event order, command return, first post-open values, effective settings and external status.

**Outcome → design.**

- Old `<v>` survives Save: compatible save admits stale caches.
- Only the true option updates the arithmetic chain: calculation is conditional.
- BeforeSave changes affect which values reach the file: event ordering belongs to the save contract.
- Opening preserves or replaces caches depending on mode/flags: implement an explicit load policy.
- Save finishes while an external operation remains outstanding: the receipt must say which result view was serialized.

### E10 ★ — Async identity, continuation, supersession and completion boundaries

**Setup.** A local native AsyncEcho(input, latch) deep-copies arguments, retains its Excel handle and waits outside Excel for a controller-owned latch. Use a formula with separately identifiable effects around the async expression and a dependent cell.

Begin with `Before()+AsyncEcho(A1,latch)+After()`, but **measure actual operand order first**; textual placement alone does not prove that Before or After runs on the intended side of suspension. A nested variant is `After(AsyncEcho(Before(A1),latch))`, with identity-valued counting functions and independently verified baseline behavior.

**Operations.** Start through COM and UI F9 separately. Release the latch normally; then test permitted edits, cancellation and close. Attempt two async call sites in one formula and a superseding call from the same cell/provider; record whether Excel permits overlap, serializes or refuses the attempt. If two calls become outstanding, release old/new results in reverse order. Include a duplicate return.

The latch controller must not require entry to Excel's blocked STA to release the result.

**Record.** Native entry/exit, privately recorded handle identity, copied arguments, prefix/suffix effects, both xlAsyncReturn status and Boolean result, CalculationState, relevant VBA/XLL events, method-return time, final caches and saved results.

**Outcome → design.**

- A prefix survives while only the remaining evaluation runs: continuation state is observable.
- A new full semantic evaluation occurs: allocate new identity; do not misclassify it as a scheduler retry.
- Stale or duplicate handles are rejected: discard and release the associated operation correctly.
- An old return is accepted in a particular state: preserve that measured policy rather than assuming all stale returns fail identically.
- COM and UI lifecycle events differ: the supervisor must not wait for an event the entry route does not provide.

---

## 6. Additional experiments needed for the remaining design decisions

These cases use the same logging, provenance and control protocol. Each is a family of small fixtures; do not combine every factor into one large workbook before the individual behavior is understood.

### E11 — Formula dialect and save/load routes

**Setup/operations:** A1:A4 contain 1,4,9,16. On separate sheets set B2.Formula and B2.Formula2 to the identical text `=SQRT(A1:A4)`. Compare explicit `=SQRT(@A1:A4)`, a legacy CSE array in B2:B5, paste, FillDown and file-load entry. Save/reopen in both controlled modes.

**Record:** Formula, Formula2, both R1C1 forms where available, Value2, HasArray, available spill properties, SavedAsArray and raw formula/array/extension metadata.

**Outcome → design:** Scalar 2 versus an array 1,2,3,4 is the expected dialect discriminator from the documented evaluation rules; actual results and rewrites are the test record. Different semantics or serialization forbid text-only coalescing. Mixed SavedAsArray output requires a tri-state interpretation. Unexpected values require checking overlap, source row and implicit-intersection placement before changing the model.

### E12 — Volatility matrix and runtime requests

**Setup/operations:** One candidate per clone: every requested volatile function, each supported CELL/INFO argument, matched/mismatched SUMIF ranges, and INDEX as a comparison. Add a dependent nonvolatile counter and unrelated control. Cross clean calculation, unrelated edit, same-value input write, selected range/sheet, Full and Rebuild under all modes.

Replay Appendix C's request-order cases: true/false, false/true, volatile call followed by an ordinary call, untaken branch, nested helper, and a conditional request that disappears on a later evaluation. Compare `IF(TRUE,1,RAND())` with an untaken volatile UDF.

**Record:** Values, call counts, explicit request trace, dependent calls, mode and entry-time calls.

**Outcome → design:** Static built-in volatility and executed UDF requests may require different mechanisms. Conditional or argument-specific results require parameterized metadata. Equal TODAY/INFO values are not evidence of skipped calculation; the observer counters are evidence about the instrumented case only. A request outside a worksheet UDF belongs in a separate no-effect control.

### E13 — Branch evaluation, unused expressions and error order

**Setup/operations:** Trace(tag,value) logs and returns its argument. Compare arithmetic operands, scalar IF, IFERROR, IFNA, CHOOSE, IFS, SWITCH, AND and OR. Repeat with array selectors, nested LET, an unused LET binding, unused LAMBDA arguments, and failures in each branch. Use plain error-expression controls alongside the UDF versions.

**Record:** Per-call-site order/count, errors, array shape and final values, using both formula dialects where applicable.

**Outcome → design:** Observed short-circuiting forbids eager execution of that branch; observed eager calls forbid dropping their effects. A suppressed error alone does not prove its expression was never evaluated. Distinguish cell expression order from cell scheduling, and do not generalize scalar behavior to lifted arrays. This test defines safe hoisting, inlining and common-subexpression elimination boundaries.

### E14 — LAMBDA recursion, helper traversal and parameter representation

**Setup/operations:** Define Down as `=LAMBDA(n,IF(n=0,0,Down(n-1)))`. Search for the largest successful depth, then vary parameter count, wrappers, mutual recursion and tail/non-tail forms. Repeat MAP, BYROW, REDUCE and SCAN with a sequence witness; compare a reused LET-bound value against a body that computes a fresh value. Add reference-consuming expressions inside a lambda parameter to distinguish references from arrays.

**Record:** Exact errors, accepted depth, argument types where observable, helper result arrays and witness order. Keep pure recursion separate from recursion instrumented with UDFs.

**Outcome → design:** Depth depending on expression shape requires a measured compatibility policy, not one folklore constant. Traversal/order differences constrain parallel helper evaluation. Captured values and callable bodies need different reuse rules. A reference-sensitive result requires preserving reference identity through the relevant parameter path.

### E15 — Dynamic references and invalidation beyond observed reads

**Setup/operations:** B1=10, C1=20, D1="B1"; F1=`=INDIRECT(D1)`, G1=`=F1+1`. Compare OFFSET, reference-valued INDEX, explicit references and an IF selecting B1/C1. Change the selector, calculate narrowly, then edit the old and new targets separately. Include an unused explicit UDF argument and a hidden-read UDF counterpart.

**Record:** Values and calls after each edit, dynamic target selection, stale-cache stages, later ordinary/full repair and binding metadata available through the public interfaces.

**Outcome → design:** A newly selected target must enter whatever dependency policy Excel exhibits; the old target may or may not continue to invalidate. Recalculation of volatile INDIRECT/OFFSET alone cannot reveal whether an old edge remains: use the nonvolatile INDEX/IF/UDF lanes to isolate that question. An unused explicit reference that still causes invocation refutes an exclusively executed-read dependency graph. A hidden callback read that observes changes without scheduling invalidation stays an observation. Keep discovery/retry policy independent of long-lived dependency recording.

### E16 — Defined names, shadowing and per-use evaluation

**Setup/operations:** Workbook name N points to Trace("global",10); cells use `=N+N` and `=N`. Add a worksheet-local N returning 20 after the existing formulas have calculated, then delete it. Repeat with constants, references, RAND, OFFSET, unused names and named LAMBDAs; create relative-reference names from different active cells.

**Record:** Old and newly entered formula results, name scope/RefersTo, invocation counts, caller, active context and save metadata. Log any calls caused by creating/editing the name itself.

**Outcome → design:** Different old/new bindings expose a resolution/invalidation rule that must be reproduced. Repeated effects forbid per-name-per-tick memoization. Relative-reference results require caller/definition context in binding identity. Unused names that are only evaluated by an explicit diagnostic command must not acquire workbook work merely because DNA Calc displays them in a browser or notebook.

### E17 — Iteration stopping, errors and changing cycle membership

**Setup/operations:** Extend E07 with `A1=A1+d`, d=1/8, and MaxChange just below, equal to and above 1/8. Add a second recurrence with a smaller change and a downstream amplifier. Separately test divergent growth, 0/1 oscillation, stable/changing strings, booleans and errors. Toggle a branch or INDIRECT target to add/remove a cycle; calculate only part of the cycle. Turn iteration off in separate runs.

Those external toggles test changes between commands. For a separate within-command lane, use an evolving selector: T1=`=IF($D$1,MIN(T1+1,3),0)`, Z1=8, B1=`=INDIRECT(IF(T1<2,"Z1","C1"))`, C1=`=B1/2`. Establish and verify seeds with D1=0, then set D1=1 in Manual. Record whether and when the dynamic target changes during the sweeps; do not assume the conditional graph was previously acyclic.

**Record:** Accepted settings, seeds, per-evaluation values, counts, warnings, errors, retained caches and repeated-command behavior without edits.

**Outcome → design:** Threshold equality distinguishes strict/weak tests; an amplifier affecting termination reveals the participant set for convergence. Continuing from previous caches differs from resetting seeds. Error or text transitions may require nonnumeric stopping rules. Membership changing mid-command requires session reconstruction without accidental effect replay. Keep diagnostics distinct from cached values when iteration is disabled.

### E18 — MTC eligibility, callback permission and thread safety versus purity

**Setup/operations:** Register TSSeq with `$`, returning an atomic increment, and a pure native workload that logs into per-thread buffers. Fill independent branches and dependent chains. Reset counters after setup and force calculation, for example with CalculateFull, for each MTC configuration; otherwise a clean nonvolatile TSSeq may not run. Compare MTC disabled, one thread and several threads; add one VBA/INDIRECT branch. Exercise allowed/unsafe C API calls under ordinary, `$` and `#` registrations.

**Record:** OS thread IDs, overlap, cell-to-counter mapping, native API status plus value tag, entry history and calculation duration.

**Outcome → design:** Changing counter assignments demonstrate order sensitivity despite race-free implementation. Workers running beside a main-thread branch refute whole-workbook serialization. Raw callback failures settle the documented code discrepancy for the tested API/context. A main-thread execution in one run is not proof that a function is universally ineligible for workers. Keep scheduler optimization independent of within-formula numerical operation order.

### E19 — RTD notification, admission and recalculation

**Setup/operations:** A local deterministic RTD provider publishes controller-triggered integers. A1 subscribes; B1 is a stateful dependent. Duplicate the topic in another cell and add a second topic. Send same-value updates, changed values and bursts; test all modes, edit mode and long calculation. Save, close, reopen, reconnect and change topics.

**Record:** ConnectData IDs and GetNewValues, UpdateNotify calls, RefreshData batches, disconnects, caches, dependent counts, events, throttle and heartbeat settings. Verify that an equal-value topic update actually appears in the RefreshData payload: server-wrapper suppression must not be attributed to Excel. Restore the original throttle after the fixture.

**Outcome → design:** Coalesced notifications are not individually committed workbook revisions. Duplicate-topic sharing affects subscription ownership. Equal-value updates that still invoke a dependent constrain cutoff. Manual-mode receipt, queueing and publication are separate decisions. Close/reopen must not confuse an old topic lifecycle with a new one. Freeze the provider for a finite snapshot test; do not wait for a perpetual stream to “finish.”

### E20 — UI, formatting and ambient-environment observations

**Setup/operations:** CELL("width",A1), CELL("format",A1), CELL("address") without a reference, INFO("origin"), INFO("recalc"), explicit-reference controls and counters. Independently vary programmatic width, divider drag, AutoFit, row hide, column hide, font, number format, zoom, scrolling, selection, active workbook, filename and calculation mode.

Enter the width formula through Formula2 in an unobstructed area; use Formula in a separate dialect-control lane so implicit intersection does not conceal the two-item shape.

**Record:** Values and shapes before explicit calculation and after ordinary/full calculation; window origin, selection, events and counters. Current CELL documentation describes a two-item width result, so record its shape as well as its first number.

**Outcome → design:** Immediate recalculation, a changed future observation and a rendering change are separate classifications. A later INFO result changing after scrolling means view state is semantic input even if no immediate calculation occurred. Interest updates must remain distinct from observable UI changes. Only measured observer-free cases justify presentation-only invalidation.

### E21 — Retained Range objects and callback generation

**Setup/operations:** A VBA module in a separate controller workbook or add-in that stays open stores an argument Range, then reads it from a later no-argument UDF. Between calls insert/delete/cut/move cells, change active workbook, create multi-area ranges and close/reopen the Range's owning workbook. Compare the stored object with a newly obtained Range. Reset the holder only between independent trials; closing its module would destroy the state being tested.

**Record:** Parent identity, external Address, Areas, Value2, TypeName/VarType, VBA errors, module state and whether ordinary calculation invokes the reader.

**Outcome → design:** Coordinate tracking, object invalidation and dependency registration are distinct policies. Never silently bind a stale handle to a newly opened workbook just because its name matches.

**DNA Calc-only extension:** Suspend a CalcVBA invocation, replace F(Double) with F(String), then resume and call a retained handle. Record code/state generation and reclamation. Excel observations inform object behavior, but they cannot certify the safety of DNA Calc's hot-reload ABI. That requires the adapter's own lifecycle test.

### E22 — External-source, link-cache and cell-cache versions

**Setup/operations:** Source.xlsx has A1=1, B1=`=A1*10`; Destination links to source B1 and adds 1. Save settled copies. Produce a source with A1=2 but saved B1=10 using Manual/no-pre-save-calc, plus another source with recalculated B1=20.

Open destination with UpdateLinks=0 and 3, calculate at each scope, explicitly UpdateLink, and reopen. Vary source closed, open in the same instance, open in another process, missing and changed on disk. Include external reference-valued names and a dynamic-array link.

**Record:** Source live/saved states, destination external-link XML and cell caches, prompts, update errors, admitted values and later calculations.

**Outcome → design:** Recalculation against 10 can be complete relative to an old snapshot while 20 exists externally. Refresh may import a source's own stale cache. Define separate source-refresh, snapshot-admission and formula-calculation operations. Unsupported external array/function paths must preserve their observed errors rather than pretending every closed-workbook reference is an ordinary local dependency.

### E23 — Open order, flags, calculation versions and open events

**Setup/operations:** Save copies with clean/dirty arithmetic, NOW/RAND, missing UDF, external link and a table. Compare first-open versus second-open behavior in a fresh instance, current-build versus genuine older-build files, and synthetic calcPr/sheetCalcPr flag variants. Set UpdateLinks explicitly. Repeat with Workbook_Open disabled, logging only, and a handler that deliberately edits/calculates.

**Record:** Original bytes before open, actual settings before/after, initial values, invocation/event sequence, prompts and post-save flags.

**Outcome → design:** The saved mode does not independently control a later-opened workbook. A flag can request work without proving it overrides Manual mode; measure precedence. Distinguish Excel's own load calculation from macro-created work. Synthetic calcId changes are useful tests of flags but do not prove equivalence to a workbook emitted by an actual old version.

### E24 — Precision as displayed and rounding boundaries

**Setup/operations:** On disposable copies use literal 1.234567 and a dependent multiplying by 1,000; formula `=1/3` and a dependent multiplying by 3. Vary explicit formats 0.00/0.0000, percentages, scientific, fractions, dates and General. Enable PrecisionAsDisplayed, inspect immediately and after calculation, change formats, then disable it and increase decimals.

Include the direct expression `=(1/3)*3` beside B1=`=1/3`, B2=`=B1*3`, with matching final-output formats. This control distinguishes possible expression-intermediate rounding from rounding at a stored cell boundary.

Separately vary width/font/zoom under General, leaving explicit formats unchanged.

**Record:** Live Value2/Value/Text, types, calls, saved numeric tokens, settings and formula results.

**Outcome → design:** Lost literal digits require a semantic data mutation. Comparing intermediates and dependents locates rounding at cell/result boundaries versus expression intermediates. Number-format-triggered changes enter invalidation. Do not infer General-width arithmetic from the option's name; let the separate test decide. Keep all original full-precision fixtures intact.

### E25 — 1904 dates and transfer/coercion routes

**Setup/operations:** Paired books with Date1904 false/true; raw serials 0,1,59,60,61,1462; DATE(1904,1,1), DATE(2026,10,31), TODAY; date/time formats and numeric controls. Toggle the setting in each mode. Transfer through Value2, Value, clipboard Range.Copy/Paste, sheet copy and external links. Compare a VBA Double parameter with a Date parameter.

**Record:** Raw doubles, Variant subtype, formatted text, formulas, saved serials and workbookPr date1904.

**Outcome → design:** Equal serials can have different date interpretations. A date-producing formula can require reevaluation after a context change even when a literal serial stays fixed. Route-dependent conversion forbids an unconditional +1,462 rule in the object library. Preserve the 1900 compatibility anomaly at the correct boundary rather than substituting the host language's calendar.

### E26 — Formula entry, explicit evaluation and command eligibility

**Setup/operations:** In Manual mode, compare setting a formula, setting the same formula again, assigning the same input via Value and Value2, entering a multi-cell formula batch and marking Range.Dirty. Compare explicit Application.Evaluate, Worksheet.Evaluate and WorksheetFunction calls with reading Value2. In a UI lane distinguish F9 while editing formula text from ordinary F9.

Add clean unused names, disabled sheets and ForceFullCalculation to individual clones.

**Record:** Calls made during the operation itself, returned types/values, ensuing ordinary/full behavior and effective settings.

**Outcome → design:** Manual cannot be implemented as “no formula execution until a Calculate command.” Entry or explicit evaluation can be its own semantic locus. Evaluate must not accidentally reuse a cached grid value merely because the text matches a cell formula. A diagnostic that evaluates a name/UDF can have effects and should be visibly distinguished from a passive inspector.

### E27 — Event-visible caches and reentrant edits

**Setup/operations:** Two dependent sheets and one independent dirty branch. Log Worksheet_Calculate, Workbook_SheetCalculate, Application_AfterCalculate and Worksheet_Change. Initially handlers only capture caches; in another lane one guarded handler edits a precedent once. Run range, sheet, all/full commands, async completion and RTD updates.

**Record:** Handler entry/exit order, values visible at each event, CalculationState, nested calculations, initiating command return and follow-up work.

**Outcome → design:** Events seeing different completion frontiers require event-specific read views. Do not emit Worksheet.Change for an ordinary result commit merely because a displayed value changed. A handler-triggered edit needs its own revision/cause and may invalidate the initiating command's stronger freshness claim. Event completion is not interchangeable with a UI repaint.

### E28 — User cancellation, scheduler yields and expensive calls

**Setup/operations:** A long native UDF that polls xlAbort; a finite version that does not; a long pure chain; an async latch and a data table. Start via UI/COM in separate lanes, issue the configured interruption action, then inspect and calculate again. In DNA Calc also queue an edit and an interest change during a single expensive formula.

**Record:** Accepted cancellation, completed cache prefix, error/partial output, next-call counts, outstanding handles, event/return sequence and longest non-yielding span.

**Outcome → design:** A cancelled Excel command that restarts later cannot be represented solely as an internal yield. Preserved partial results determine permitted publication. A finite WorkBudget does not bound an opaque native call's duration; admission and UI responsiveness must reflect that limit. Numeric kernels need safe interruption boundaries; do not treat arbitrary instruction interruption as a tested numeric-equivalence mechanism.

### E29 — Formula transformations and binding after structural edits

**Setup/operations:** A fixture with relative/absolute/mixed references, ranges, local/global names, structured references, `@`, `#`, legacy arrays and external references. Independently copy, cut/move, partially insert/delete, insert whole rows/columns, copy/move sheets and perform interactive Undo where available. Include edits crossing a spill boundary and a retained Range from E21.

**Record:** Both formula dialects, rewritten references/names, values, object addresses, errors, spill/array metadata and saved representation after each operation.

**Outcome → design:** Each operation defines a transformation rule; one row-insert rule does not establish cut/move or sheet-copy semantics. Observed binding changes require invalidating resolution provenance. An unsupported Undo route through programmatic edits is not a semantics failure—record that limitation and use the UI operation where the workbook's Undo behavior is the subject.

### E30 — Performance and lifecycle measurements outside Excel semantics

Appendix B includes engineering claims that black-box Excel traces cannot establish. Keep these in DNA Calc's benchmark/lifecycle suite:

| Claim | Concrete workload/operation | Record and design decision |
|---|---|---|
| Names scale under retained snapshots | 10k, 100k, then larger name sets; edit one while retaining closures/snapshots. | Copied bytes, retained generations, rebind count. Superlinear copying rejects the storage strategy; correctness still uses E16/E29. |
| Typed VBA path is inexpensive | Empty/scalar/string/array calls, then one Range callback, interpreter and native routes. | Allocations, conversion bytes, latency distribution. Compare equal semantics; do not trade away reference identity silently. |
| Explicit function context is affordable | Arithmetic, large ranges, name-heavy and callback-heavy workloads. | Dispatches, bytes/read, allocations and throughput; identify the actual dominant cost. |
| Block storage survives edits | Dense fill, sparse membership, scattered edits and repeated insert/undo. | Peak memory, fragmentation, copied blocks and saved size. |
| Progressive tiles remain coherent | Multi-tile spills, slow subscriber, dictionary rotation and reset. | Atomic consumer state, bounded retention and mismatch recovery. A visually coherent tile is not itself a freshness certificate. |
| Runtime/adapter lifecycle improves | Provider churn, CPU load, edit/cancel/reload, retained handles. | Failures, responsiveness, release/reclamation, binary/build cost. |
| Spill barriers stay selective | E06's increasing uncertain-anchor ladder. | Candidate blockers and first-settled latency; distinguish proof delay from calculation speed. |

Use operation counters over geometric sizes, then targeted timing. These are explicit investigation runs, not a new mandatory large per-commit check.

---

## 7. Invariants safe enough to build on

The list deliberately separates Excel evidence from conditional mathematical/design guarantees. No item asserts an undocumented universal scheduling order.

| Invariant | Evidence | Boundary |
|---|---|---|
| Formula dialect and reference/array distinction must be preserved. | **D**, §2.6. | Equal formula text alone does not establish equivalence. |
| A command receipt describes the operation actually completed. | **D + I**, scoped API contracts and E02's required provenance. | It is not a workbook-wide freshness certificate. |
| Permitted cached observations must not be silently replaced by fresh-demand reads. | **D + O**, §2.3 and Appendix C. | Read classes differ; exact values/order need E01/E03/E04. |
| Physical retries must not invent extra effects; required semantic reinvocations must remain possible. | **D + I**, native callback protocol plus invocation accounting. | Resume and unwind/reinvoke are different states. |
| Value equality alone is insufficient to suppress a stateful dependent. | **O**, Appendix C. | Pure-cone cutoff needs an additional observational-equivalence proof. |
| A fresh blank read requires valid occupancy/absence information. | **I**, a consequence of spill production. | Conservative bounds can be broad; selectivity is measured separately. |
| File caches carry load/save provenance, not universal currentness. | **D + I**, §2.13. | Reopening can change the values one is trying to inspect. |
| Async calls, RTD subscriptions and link snapshots have different identities and lifetimes. | **D + I**, §2.11–2.12. | Completion policy is specific to each kind of operation. |
| Context affecting interpretation belongs in validity: bindings, precision, dates, metadata and permitted environment reads. | **D + I**, relevant APIs/functions above. | Not every context change triggers immediate recalculation. |
| Dependency-respecting reorder is safe for an isolated pure acyclic region under the assumptions below. | **Mathematical conditional invariant**, not an empirical Excel order claim. | Arbitrary UDFs, unstable occupancy, observable intermediate caches and nondeterminism are excluded unless explicitly controlled. |

The last statement can be made precise:

> Fix the command, bindings, input snapshot, semantic settings and within-formula numeric/evaluation behavior. Suppose each node in a finite acyclic region is a deterministic, effect-free function of its declared inputs, all relevant occupancy is established, and no permitted observer can distinguish intermediate publication. Then any dependency-respecting evaluation order produces the same final values.

This follows by induction over the dependency graph. It permits interest-based scheduling in that region. It does **not** permit changing reduction order inside SUM-like computations, collapsing effectful invocations, sampling time/randomness differently, or exposing the region's intermediate results to a hidden observer.

For cyclic regions, data tables and competing spills, establish the session-level transition rules first; the DAG proof does not extend to them automatically.

---

## 8. Suggested implementation sequence and decisions the probes unlock

1. **First establish E01, E02 and E04.** These select the read protocol, command receipts and invocation/reinvocation state machine. Use the existing Appendix C fixtures as the starting evidence.
2. **Run E03, E05 and E06 next.** These select observation-view fencing, nondeterministic occurrence identity and spill publication/absence policy.
3. **Run E07–E10 before treating special sessions as complete.** They determine iteration participation, scenario state, save boundaries and external completion lifetimes.
4. **Run the corresponding E11–E29 cases as each capability is admitted.** Unsupported edges remain explicit compatibility gaps; they are not silently “fixed” by stronger freshness or fewer evaluations.
5. **Use E30 only to choose between semantically acceptable implementations.** Preserve the fast loop of concrete regression fixtures and inexpensive contract invariants.

For every result, retain a short decision record: what was observed, which competing models were excluded, which settings/history it depends on, and what remains unknown. A serial DNA Calc reference engine is valuable for regression and schedule exploration, but its chosen order is not evidence of Excel's order.

The strongest attainable foundation is therefore a small, explicit operational model backed by pinned traces, with broad optimization inside proven observer-free regions. That supports early visible results without making freshness or callback behavior accidental properties of the scheduler.

---

## 9. Sources and provenance

Public sources were checked for this review on 31 October 2026. Microsoft pages with older applicability labels are deliberately not treated as exhaustive current-build specifications. The R sources are original practitioner reports, not secondary retellings.

The supplied **Appendix C in `4-calc-semantics-prompt.md`** is the source of every **O** claim. Source code paths and fixture names in that appendix identify retained evidence in the user's project; they are not assertions that those files were accessed during this review.

### Recalculation, commands and object-model values

- [MS01] Microsoft, *Excel Recalculation*.
- [MS02] Microsoft, *Excel performance: Improving calculation performance*.
- [MS03] Microsoft / public SpreadsheetML specification, *Working with the calculation chain*.
- [MS04] Microsoft, *How Excel determines the current mode of calculation*.
- [MS05] Microsoft, *XlCalculation enumeration*.
- [MS06] Microsoft, *Application.Calculate method*.
- [MS07] Microsoft, *Worksheet.Calculate method*.
- [MS08] Microsoft, *Application.CalculateFull method*.
- [MS09] Microsoft, *Application.CalculateFullRebuild method*.
- [MS10] Microsoft, *Range.Dirty method*.
- [MS11] Microsoft, *Change formula recalculation, iteration, or precision in Excel*.
- [MS12] Microsoft, *Worksheet.EnableCalculation property*.
- [MS13] Microsoft, *Workbook.ForceFullCalculation property*.
- [MS14] Microsoft, *Range.Value2 property*.
- [MS15] Microsoft, *Range.Value property*.
- [MS62] Microsoft, *Application.CalculationState property*.
- [MS63] Microsoft, *Range.CalculateRowMajorOrder method*.

### UDFs, volatility, arrays and formula language

- [MS16] Microsoft, *Description of limitations of custom functions in Excel*; applicability includes Excel 2010.
- [MS17] Microsoft, *Excel Commands, Functions, and States*.
- [MS18] Microsoft, *xlfRegister (Form 1)*.
- [MS19] Microsoft, *Excel4/Excel12*.
- [MS20] Microsoft, *Application.Volatile method*.
- [MS21] Microsoft, *RAND function*.
- [MS22] Microsoft, *RANDARRAY function*.
- [MS23] Microsoft, *Dynamic array formulas and spilled array behavior*.
- [MS24] Microsoft, *How to correct a #SPILL! error*.
- [MS25] Microsoft, *Formula vs Formula2*.
- [MS26] Microsoft, *Implicit intersection operator: @*.
- [MS27] Microsoft, *Range.SavedAsArray property*.
- [MS28] Microsoft, *LET function*.
- [MS29] Microsoft, *LAMBDA function*.
- [R01] Charles Williams, *Short-circuiting Excel Formulas: IF, CHOOSE, IFS and SWITCH*, 3 January 2023, with an IFNA update noted in August 2025.
- [R02] Charles Williams, *Short-circuiting in Array Formulas*, 6 January 2023.

### Iteration, tables, threading and external operations

- [MS30] Microsoft, *Application.Iteration property*.
- [MS31] Microsoft, *Application.MaxIterations property*.
- [MS32] Microsoft, *Application.MaxChange property*.
- [MS33] Microsoft, *Excel performance: Tips for optimizing performance obstructions*.
- [MS34] Microsoft, *Range.Table method*.
- [MS35] Microsoft, *Known Issues in Excel XLL Development*.
- [R03] Charles Williams / Decision Models, *Excel Calculation Process*; includes historical version-specific observations.
- [MS36] Microsoft, *Multithreaded recalculation in Excel*.
- [MS37] Microsoft, *Asynchronous user-defined functions*.
- [MS38] Microsoft, *xlAsyncReturn*.
- [MS39] Microsoft, *IRtdServer interface*.
- [MS40] Microsoft, *IRTDUpdateEvent.UpdateNotify method*.
- [MS41] Microsoft, *IRtdServer.RefreshData method*.
- [MS42] Microsoft, *RTD function*.
- [MS43] Microsoft, *RTD.ThrottleInterval property*.
- [MS44] Microsoft, *Workbooks.Open method*.
- [MS45] Microsoft, *Workbook.UpdateLink method*.
- [MS46] Microsoft, *External links may be calculated when you open a workbook that was last saved in an earlier version of Excel*.

### Persistence, semantic settings and events

- [MS47] Microsoft, *Application.CalculateBeforeSave property*.
- [MS48] Microsoft / public SpreadsheetML specification, *Working with formulas*.
- [MS49] Microsoft Open XML SDK, *CalculationProperties class*.
- [MS50] Microsoft, *Workbook.PrecisionAsDisplayed property*.
- [MS51] Microsoft, *Workbook.Date1904 property*.
- [MS52] Microsoft, *Date systems in Excel*.
- [MS53] Microsoft, *Excel incorrectly assumes that the year 1900 is a leap year*.
- [MS54] Microsoft, *CELL function*.
- [MS55] Microsoft, *INFO function*.
- [MS56] Microsoft, *Worksheet.Change event*.
- [MS57] Microsoft, *Worksheet.Calculate event*.
- [MS58] Microsoft, *Application.AfterCalculate event*.
- [MS59] Microsoft, *Application.CalculateUntilAsyncQueriesDone method*.
- [MS60] Microsoft, *Handling Events* for XLLs.
- [MS61] Microsoft, *Permitting User Breaks in Lengthy Operations*.

[MS01]: https://learn.microsoft.com/en-us/office/client-developer/excel/excel-recalculation
[MS02]: https://learn.microsoft.com/en-us/office/vba/excel/concepts/excel-performance/excel-improving-calculation-performance
[MS03]: https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/working-with-the-calculation-chain
[MS04]: https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/current-mode-of-calculation
[MS05]: https://learn.microsoft.com/en-us/office/vba/api/excel.xlcalculation
[MS06]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.calculate
[MS07]: https://learn.microsoft.com/en-us/office/vba/api/excel.worksheet.calculate(method)
[MS08]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.calculatefull
[MS09]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.calculatefullrebuild
[MS10]: https://learn.microsoft.com/en-us/office/vba/api/excel.range.dirty
[MS11]: https://support.microsoft.com/en-us/excel/change-formula-recalculation-iteration-or-precision-in-excel
[MS12]: https://learn.microsoft.com/en-us/office/vba/api/excel.worksheet.enablecalculation
[MS13]: https://learn.microsoft.com/en-us/office/vba/api/excel.workbook.forcefullcalculation
[MS14]: https://learn.microsoft.com/en-us/office/vba/api/excel.range.value2
[MS15]: https://learn.microsoft.com/en-us/office/vba/api/excel.range.value
[MS16]: https://support.microsoft.com/en-us/excel/description-of-limitations-of-custom-functions-in-excel
[MS17]: https://learn.microsoft.com/en-us/office/client-developer/excel/excel-commands-functions-and-states
[MS18]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlfregister-form-1
[MS19]: https://learn.microsoft.com/en-us/office/client-developer/excel/excel4-excel12
[MS20]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.volatile
[MS21]: https://support.microsoft.com/en-us/excel/functions/rand-function
[MS22]: https://support.microsoft.com/en-us/excel/functions/randarray-function
[MS23]: https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior
[MS24]: https://support.microsoft.com/en-us/excel/how-to-correct-a-spill-error
[MS25]: https://learn.microsoft.com/en-us/office/vba/excel/concepts/cells-and-ranges/range-formula-vs-formula2
[MS26]: https://support.microsoft.com/en-us/excel/implicit-intersection-operator
[MS27]: https://learn.microsoft.com/en-us/office/vba/api/excel.range.savedasarray
[MS28]: https://support.microsoft.com/en-us/excel/functions/let-function
[MS29]: https://support.microsoft.com/en-us/excel/functions/lambda-function
[MS30]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.iteration
[MS31]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.maxiterations
[MS32]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.maxchange
[MS33]: https://learn.microsoft.com/en-us/office/vba/excel/concepts/excel-performance/excel-tips-for-optimizing-performance-obstructions
[MS34]: https://learn.microsoft.com/en-us/office/vba/api/excel.range.table
[MS35]: https://learn.microsoft.com/en-us/office/client-developer/excel/known-issues-in-excel-xll-development
[MS36]: https://learn.microsoft.com/en-us/office/client-developer/excel/multithreaded-recalculation-in-excel
[MS37]: https://learn.microsoft.com/en-us/office/client-developer/excel/asynchronous-user-defined-functions
[MS38]: https://learn.microsoft.com/en-us/office/client-developer/excel/xlasyncreturn
[MS39]: https://learn.microsoft.com/en-us/dotnet/api/microsoft.office.interop.excel.irtdserver?view=excel-pia
[MS40]: https://learn.microsoft.com/en-us/dotnet/api/microsoft.office.interop.excel.irtdupdateevent.updatenotify?view=excel-pia
[MS41]: https://learn.microsoft.com/en-us/dotnet/api/microsoft.office.interop.excel.irtdserver.refreshdata?view=excel-pia
[MS42]: https://support.microsoft.com/en-us/excel/functions/rtd-function
[MS43]: https://learn.microsoft.com/en-us/office/vba/api/excel.rtd.throttleinterval
[MS44]: https://learn.microsoft.com/en-us/office/vba/api/excel.workbooks.open
[MS45]: https://learn.microsoft.com/en-us/office/vba/api/excel.workbook.updatelink
[MS46]: https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/external-links-calculated-open-workbook-earlier-version-excel
[MS47]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.calculatebeforesave
[MS48]: https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/working-with-formulas
[MS49]: https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.calculationproperties?view=openxml-3.0.1
[MS50]: https://learn.microsoft.com/en-us/office/vba/api/excel.workbook.precisionasdisplayed
[MS51]: https://learn.microsoft.com/en-us/office/vba/api/excel.workbook.date1904
[MS52]: https://support.microsoft.com/en-us/excel/date-systems-in-excel
[MS53]: https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/wrongly-assumes-1900-is-leap-year
[MS54]: https://support.microsoft.com/en-us/excel/functions/cell-function
[MS55]: https://support.microsoft.com/en-us/excel/functions/info-function
[MS56]: https://learn.microsoft.com/en-us/office/vba/api/excel.worksheet.change
[MS57]: https://learn.microsoft.com/en-us/office/vba/api/excel.worksheet.calculate(even)
[MS58]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.aftercalculate
[MS59]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.calculateuntilasyncqueriesdone
[MS60]: https://learn.microsoft.com/en-us/office/client-developer/excel/handling-events
[MS61]: https://learn.microsoft.com/en-us/office/client-developer/excel/permitting-user-breaks-in-lengthy-operations
[MS62]: https://learn.microsoft.com/en-us/office/vba/api/excel.application.calculationstate
[MS63]: https://learn.microsoft.com/en-us/office/vba/api/excel.range.calculaterowmajororder
[R01]: https://fastexcel.wordpress.com/2023/01/03/short-circuiting-excel-formulas-if-choose-ifs-and-switch/
[R02]: https://fastexcel.wordpress.com/2023/01/06/short-circuiting-in-array-formulas/
[R03]: https://www.decisionmodels.com/calcsecretsc.htm



