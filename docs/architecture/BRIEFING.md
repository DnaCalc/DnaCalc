# DNA Calc target architecture review: briefing pack

Date: 2026-10-31. Audience: the three drafting models (Opus 5.5, Fable 5.1, GPT-Astra 6), their
mutual reviewers, and a final GPT-6 Pro red-team. Everything you need to start is here or in the
files this pack names. Read this whole file first.

## 1. What is asked

Produce a target architecture for DNA Calc that prepares it for **extreme flexibility and extreme
optimisation**, worked out first at an abstract level and then reflected down onto the current
Rust implementation (crates, boundaries, data representations), with an implementation strategy
that gets there from today's code. The value of this exercise is in the **non-obvious seam
pressures**: places where two concerns meet and the convenient design quietly breaks something
later. Generic layered-architecture advice is worthless here; ground every claim in the scope
below and in the evidence of what has already gone wrong.

Process: three independent drafts; each model then reviews the other two; a synthesis keeps
disagreements visible; one more review cycle; then a GPT-6 Pro red-team. Do not read other
models' drafts until you are asked to review them.

## 2. Product and scope

- **DNA Calc**: a re-implementation of an Excel-compatible spreadsheet in Rust, running natively
  (Windows, Linux) and in the browser (wasm in web workers, Leptos UI). Goal: Excel's observable
  behaviour, including bit-identical function results (see 4.4), opening and saving real xlsx/xlsb
  files, Excel's extensibility (LAMBDA, UDFs, VBA, add-ins), with several UI skins.
- **Long-term ambition (Jane Street "Webs"-class)**: distributed computation; incremental
  recomputation over billions of nodes and edges; strong typing and rich data structures;
  definition from Rust and from a web interface; a web formula debugger; performance at that scale.
  All still on Excel formulas, functions and extensibility. Until now this was a deferred epic with
  a rule against designing toward it; **for this review the owner explicitly asks the architecture
  to be ready for it**. Implementation stays Excel-scope unless the owner places more.
- **Preferential recalculation of what is visible**: the grid viewport and anything a skin
  subscribes to should drive recalculation priority; stale work should be cancellable when the view
  moves. Rendering speed depends on suitable representations all the way from UI state to engine.
- **DNA OneCalc**: an engine-free front end on CalcFml and CalcFunc only (formulas, number
  formatting, UDF integration). It is the honesty check that the formula/function layers stay
  simple and separate from the engine, which owns dependency-graph smarts and optimisation. It is
  not a reduced skin of DNA Calc and not a parallel product. DNA Calc is a strict superset.
- **Skins**: a strict skinnable model. All functionality, state and commands live in a skinnable
  functional model; skins (Grid, Atlas, Ledger, Canvas, Tree, Notebook, OneCalc's front end) only
  present and route. Purpose: trivially agent-customisable UIs that still present full
  functionality. Shared UI lives once in a library at OneCalc's dependency ceiling.
- **CalcVBA** (now repo OxVba): an independent VBA engine with interpreter and JIT, see 4.5.

## 3. The system as built (2026-10-31)

Crates: `calc-value` -> `calc-func` (about 500 Excel functions) -> `calc-fml` (parse, bind,
compile, evaluate formulas) -> `calc-doc-model` / `calc-doc` (document model, xlsx/xlsb codecs) ->
`calc-engine` (dependency graph, recalculation) -> `calc-host` (session facade, projections, edits)
. Apps: `dna-onecalc`, `dna-calc` (Leptos UI + web-worker `server.rs`), `dna-formula-ui` (shared,
ceiling calc-fml), `dna-strand` (design tokens). `crates/calc-vba` adapts OxVba. Tools: `xtask`
(check, sweep, capture, witness, site publishing), `calc-verdict`. Platform: `calc-oracle-excel`.
Tracker `br`; master-only; Codex workers on two Linux hosts and Windows.

The original plan is `C:\Work\DnaCalc\DnaCalc-review\docs\plan\DNA_CALC_REIMPLEMENTATION_PLAN.md`
(sections 2-8 are the intended architecture; compare it with the audits). Code is at
`C:\Work\DnaCalc\DnaCalc-review` (master 5a9348b); read code to check claims, read-only.

## 4. Owner principles and rulings for this review

4.1 **Explicit context, not convenient placement.** Formatting and presentation must not live in
calc-func. Where functions need such facts (CELL, TEXT, INFO, reference resolution for
INDIRECT/OFFSET, volatile services for NOW/RAND, user-defined function lookup), calc-fml passes a
Formula Evaluation Context with information and callbacks, at a suitable abstraction. The same
holds for the layers above calc-fml: boundaries are explicit, and the full context each layer
needs is passed to it, never smuggled by moving code to a convenient but architecturally
inconsistent place.

4.2 **Skins**: the app shell has drifted from the strict skinnable model (see risk-skins.md).
Course-correct; refactor the functional model and the top-level skins.

4.3 **Representations end to end**: grid performance and visible-first recalculation require
suitable representations from UI state through the worker boundary to the engine.

4.4 **Bit-exact functions mean Excel's algorithm, not a fit.** The target is an implementation that
is the same program as Excel's (assumed straightforward human-written code, possibly assembly):
expression tree, operation order, precision and store points, constants, branches. Found by
symbolic search over candidate calculation programs, constrained by recorded Excel answers
(OxFunc's `docs/SMART_SEARCH_AND_ACTIVE_LEARNING_LOOP.md` method). Counting matches and optimising
toward a higher match ratio is the wrong method; fitted bounds, coefficients or error-based switches
are fix-up tables and are being reverted. The architecture should give this research a proper home
(where candidate programs, precision models and witnesses live) without contaminating the product.
Excel versions are assumed stable for function results; every reading records build, platform,
CPU. Measured fact: on AMD Excel follows the CPU's x87 result exactly (15 of 15 inputs where Intel
and AMD hardware differ).

4.5 **CalcVBA and DNA Calc.** CalcVBA is a separate repo and engine with nothing
spreadsheet-specific (no "UDF" concept). It exposes the right kind of external interface, as Office
VBA presumably does: load code, enumerate public module procedures and their signatures, give the
host callable entry points. DNA Calc has a glue library that registers shims for those as worksheet
functions, and builds and registers its own COM type library (Application, Range,
WorksheetFunction) that VBA code uses as it would Excel's. Owner, verbatim in substance:
- the UDF call path must be an **early-bound, high-performance call** from CalcFml/CalcFunc
  invocation through to JIT-compiled code; do not settle for "getting the right number across";
- the DNA Calc object library exposed to VBA needs **full early-binding COM through to JIT code**
  where appropriate and compatible; late binding is not a start to improve later, experience says
  that builds the wrong system; if early-bound COM is not fully baked in both interpreter and JIT,
  that is the priority before any late-bound path from DNA Calc;
- module procedures are not COM objects, but their exposure should be as close to COM as
  possible; Excel's VBA glue likely has a small non-COM step in registration and COM-like dispatch
  otherwise; understand and mirror it, do not overdo our own;
- both interfaces may be needed; Windows gets full compatibility; Linux and web a subset (no
  general COM references on Linux, no direct file I/O on the web).
Open: whether "no code cross-dependencies" forbids a Cargo dependency on a neutral CalcVBA hosting
API crate (so the boundary is a binary interface: C/COM ABI, wasm component), or allows the glue
crate alone to depend on it. Propose an answer with reasons.

4.6 **Inner loop stays tight.** No process-laden development; per-commit checks catch real defects
and stay fast; slow validation is explicit and schedulable, never something work waits on; fix
couplings at the root rather than working around them.

4.7 **Libraries.** The owner asked to consider basing the next-generation engine on `asupersync`
and similar high-value work by the same author or others. Licence (OWNER RULING, 2026-10-31,
correcting research-libraries.md, which overstates the problem): asupersync and the author's other
projects are MIT plus a rider that keeps OpenAI and Anthropic themselves from taking the code for
their own products and tooling. It does not stop us, using coding agents on our own behalf, from
depending on, reading, fixing or improving it. asupersync is acceptable as a dependency, with a
pointer to the rider in our third-party notices. The residual risk is only a future licensee of
DNA Calc that is one of those companies; to be doubly cautious, ring-fence it: a clean dependency
boundary plus a simple non-concurrent execution path we maintain ourselves, so the engine runs
without it. Evaluate asupersync on technical merit (maturity, nightly requirement, browser support,
overhead; see research-libraries.md) alongside alternatives (salsa, formualizer, Arrow,
msft-typelib, wasm components/WIT, Cranelift).

4.8 **Owner rulings during review round 1 (2026-10-31).**
- **Non-cell nodes are defined names.** Excel already needs nodes that are not cells; defined
  names (workbook- and sheet-scoped, with their affinity rules) are the first-class non-cell graph
  nodes, and the design must support very large numbers of them. A Jupyter-notebook-style front end
  over Excel formulas must be able to sit on either a cell sequence on a sheet or on defined names,
  chosen later or switched dynamically. No further kinds of non-cell node are in scope.
- **Scale target**: what Excel supports first (sheet limits, name counts, workbook sizes), with the
  architecture ready to scale; billions of cells is not a goal now.
- **No stable row/column keys baked into the architecture.** Inserting a row is disruptive in Excel
  too. Start from what Excel persists (xlsx: A1 formula text with shared-formula masters and
  ranges; xlsb: parsed token streams with relative references; note that R1C1 and A1 formulas can
  behave differently) and treat the in-memory state as an optimisable representation of that. Fast
  structural updates come from optional indexes or pointer structures for the bad cases (for
  example large formula sets that would otherwise be scanned after a structural change), not from
  overhead built into every cell and reference.

4.9 **Owner ruling on add-ins (2026-10-31).** Full .xll add-in compatibility and broad COM
object-model support are an early focus, not only VBA. Excel-DNA, xlw and xlOil should run out of
the box, perhaps with some general or add-in-specific shimming (PyXLL later; it is commercial).
The architecture must host the Excel C API (registration, callbacks, XLOPER12, caller, thread-safe
and asynchronous UDFs, RTD) and the COM object model add-ins use, on Windows.

4.10 **COM activation compatibility is an open design area (owner, 2026-10-31).** Intercepting or
virtualising COM activation inside the DNA Calc process (so in-process code that creates or gets
"Excel.Application", Excel's type library or interfaces finds DNA Calc) is worth investigating, but
the exact compatibility design stays open. Constraint: nothing may break the machine's COM
registrations for Excel: no changes to Excel's ProgIDs, CLSIDs, type-library or interface
registrations, in HKCU or HKLM. Prefer process-local mechanisms when the design is taken up.

## 5. Evidence: the foundation risk audit (read all six files)

All in `C:\Work\DnaCalc\parked\arch-review\`. Headlines:

- **risk-layering.md**: calc-fml has no evaluation context (six forwarders default to "not
  modelled"; ROW/CELL/SHEET/FORMULATEXT refused inside data tables and ROW() in OneCalc); a
  single-threaded UDF registry in calc-func headed into the multi-threaded engine; number-format
  rendering in calc-func against plan 4.4; grid and CELL derive width/alignment/sheet-name facts
  separately and disagree; locale half passed, half hard-coded; setting the clock makes every
  formula pending; conditional formats evaluated in the host over raw XML; function identity by
  spelling (volatility list, SUBTOTAL/AGGREGATE, two `_xlfn.` tables). Ends with what each
  boundary's explicit context would need.
- **risk-skins.md**: no command layer (shortcuts bound in the Grid skin, string-dispatched menus;
  Undo/Find/F9 absent in other skins); controller shaped around the Grid; worker protocol has one
  request per skin, some carrying pixel geometry and SVG; Excel rules (sheets, panes, axes, CF,
  replace, paste) live in the app's worker instead of calc-host; five copies of "show in skin X";
  a new skin touches about eight places; OneCalc has no model under its front end; god files.
- **risk-engine.md**: formulas stored as text per cell, shared formulas expanded (a 1M-row fill is
  1M graph nodes; row insert re-parses everything); recalculation runs to completion and publishes
  all at once with no priority input; cancel discards; parallel recalc native-only and disabled by
  spills/data tables; per-cell JSON with about eight strings each, viewport resent per block, DOM
  rebuilt per scroll; 600-900 B per formula cell over six maps; per-cell styles replaying edits;
  single-message worker; what is worth keeping (paged shared store, generational IDs, resumable
  recalc state, per-result calc stamp, prefix-sum geometry, worker boundary).
- **risk-vba.md**: the Excel object model lives inside OxVba (`oxvba-embed`), breaking its own
  charter; `ComCallEarly` exists but both runtimes lower it to name-keyed dynamic dispatch; the
  only vtable path is Windows x64 libffi per call; formula-to-VBA is name-keyed with three
  conversions each way and 8-10 allocations; a 4,096 host-object cap; DNA Calc's side is small.
- **risk-inner-loop.md**: check is the right shape but every edit compiles twice (xtask depends on
  product crates); app smokes (482 cases, one 8,754-line file, no case filter, pinned wording and
  pixels) grew fastest and run nowhere; 10 MB of receipts nobody reads; retired-CI tooling; four
  overlapping layering checks; tests and fixtures named after tracker beads.
- **research-libraries.md**: asupersync and related, plus alternatives.
- **foundation-ideas.md** (added for the review round): good ideas from the early maximum-ambition
  Foundation plans (Feb-Mar 2026) that were deferred or lost: per-value epochs and per-node
  publication with a replayable visibility stream; stable row/column IDs and formula blocks; an
  observer-driven necessary set with a height-ordered heap and early cutoff; declared
  evaluation-context facilities with invalidation tokens and call handles; one protocol surface;
  RenderPlan IR; counter-slope checks; non-grid nodes as graph participants.

## 6. What each draft must contain

Write to `C:\Work\DnaCalc\parked\arch-review\drafts\<model>.md`, at most 450 lines, in this order:

1. **Abstract architecture**, independent of today's code: core concepts and invariants; the
   layers and the explicit context each boundary carries (who supplies what, including callbacks);
   data representations (formulas and formula blocks, values, styles/formats, names, the dependency
   graph, IDs, partitions); the recalculation contract (demand and interest sets, priority,
   cancellation, incremental publication, determinism); concurrency and distribution; the command
   and functional model beneath skins; extension seams (LAMBDA, UDFs, CalcVBA hosting, the COM
   object library, add-ins, agents); the place of the Exact numeric path and of function research;
   platform profiles (Windows, Linux, browser).
2. **Seam pressures**: the non-obvious ones, each with the evidence that shows it is real and the
   design response. Include at least: function context vs presentation; shared formulas vs
   per-cell identity and structural edits; visible-first recalc vs Excel's calculation semantics
   (volatility, iteration, data tables, spills); worker boundary vs fine-grained reactivity;
   early-bound VBA vs portability and reload; OneCalc's ceiling vs UDF registration; skins vs one
   command model; bit-exact research vs product code.
3. **Reflection onto Rust**: the target crate map, what each crate owns, dependency rules, which
   current crates and modules move, split, merge or are rewritten, and what is kept as is.
4. **Implementation strategy**: order of work, what stays shippable at each step, the inner loop
   and test strategy consequences, library choices, and how the Codex workers' current lanes (UI
   slices, bit-exact research, CalcVBA) should be redirected.
5. **Decisions for the owner**: the few that only the owner can make, each with your
   recommendation.

Be concrete (name types, messages, invariants), skeptical, and short where the answer is simple.
Read code to verify anything you rely on; cite file:line where it matters.
