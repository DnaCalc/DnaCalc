# Foundation risk audit: layering and context leaks

Lens: where facts, services and presentation concepts cross the crate stack by convenient routes
instead of an explicit context handed down by the layer above. Clone `DnaCalc-review` at 5a9348b,
read only. Ranked by how much current or queued work builds on each problem.

## R1. calc-fml has no evaluation context; the cell reader carries the environment, host facts and caller

Evidence
- `calc-fml/src/eval/mod.rs:480-489`: `EvaluationServices` holds numeric mode, reader, observer,
  catalog, random, UDF registry, diagnostics, effects. No environment, no host facts, no caller.
- `calc-fml/src/reference/read.rs:47-55`: `ReferenceReader` (a value reader) also answers
  `environment()` (date system, locale profile, clock) and `host_facts()`. `eval/cells.rs:208-254`
  forwards them into calc-func's `CellReader`, re-borrowing the reader once per question.
- `calc-func/src/call.rs:51-62, 385-398`: calc-func reads the environment and host facts from the
  services or else from the cell reader. `with_environment` (`:364`) and `with_host` (`:380`) have
  no production caller, but `locale.rs:8-10` documents them as the route.
- Six reader wrappers re-implement the same forwarding: engine `read.rs:934/1054`,
  `recalc.rs:2201/2264`, `detached.rs:125/151`, `data_table.rs:308`, calc-fml `cells.rs:158/219`,
  OneCalc `fixture.rs:130`. Every `HostFacts` method has a default of `None`
  (`call.rs:113-169`), so a wrapper that forgets one refuses silently. It has already happened:
  `data_table.rs:308-388` (`WhatIfReader`) has no `host_facts`. Inside a data table's what-if
  evaluation, ROW(), COLUMN(), SHEET, ISFORMULA, FORMULATEXT, CELL and INFO are refused.
- The caller cell has two routes. The UDF context takes it from the instance anchor
  (`calc-fml/src/eval/run.rs:1868`). ROW()/COLUMN() and CELL take it from `HostFacts::caller`
  (`calc-func/src/catalogue/lookup.rs:1751,1768`), which the engine supplies
  (`recalc.rs:2265`, `detached.rs:152`). So ROW() is refused in OneCalc, which has no host facts,
  even though calc-fml holds the anchor.

Why it is weak: each new fact (precision as displayed, external books, RTD, add-in caller) adds
another defaulted trait method and another forwarding edit in six places. A missed edit
shows up only as a refusal in one evaluation route.
What builds on it: dc-snz.6 (in progress), dc-zmk, dc-jnt.14, CF/DV detached evaluation, data tables.
Direction: put a `FormulaEvaluationContext` in calc-fml, built once per evaluation by the caller.
It holds the caller identity (taken from the instance, never from the host), environment, host
facts, volatile services, the callable/UDF registry and effect sinks. Reduce `ReferenceReader` to
value, shape and spill access. calc-fml derives calc-func's `EvalServices` from it in one place.
Delete the fallback from services to cell reader in `call.rs:385-398`.

## R2. The UDF registry lives in calc-func, uses Rc, and is about to be carried into a multithreaded engine

Evidence
- `calc-func/src/udf.rs:5,53`: `UserFunctions` is `BTreeMap<String, Rc<dyn UserFunction>>`, which
  is not `Send`. calc-func never calls it. calc-fml compiles against it
  (`compile.rs:174-204`, `compile_with_functions`) and evaluates against it
  (`eval/mod.rs:507`, `with_functions`). That is two separate supply points for one registry, and
  nothing checks that they match.
- `calc-vba/src/lib.rs:11-13`: a `thread_local!` CODE_CACHE. `:85-86` puts the session in `Rc`
  shared by every registered function.
- `calc-engine/src/recalc.rs:968`: levels evaluate on `std::thread::scope` workers, and
  `WorkbookFacts` is explicitly `Send + Sync` (`call.rs:177`). The engine passes no UDF registry
  today (`recalc.rs:2469-2472`).
- Bead dc-snz.6 (in progress) says to carry the "caller-owned calc-func UDF registry through
  CalcFml ... CalcEngine graph into CalcHost".
- Plan §2.2 and §4.1 put the registry generation in `PlanKey`. The engine's plan and template
  caches (`state.rs:395-401`) have no such key.

Why it is weak: the type sits in the bottom crate only so calc-vba can avoid calc-fml; its thread
affinity conflicts with the engine's threading; the generation is not part of plan identity.
What builds on it: dc-snz.6, dc-snz.1/.2 (.xll), dc-snz.5, mostly still to come, so fixing it now
costs less than after it lands.
Direction: give calc-fml a `UserFunctionResolver` trait for compile-time resolution with a
generation, and a call callback for eval. Supply both through the one context in R1. Decide
explicitly how evaluation threads run: route thread-affine UDF calls to an owning thread, or
evaluate on one thread when a plan has `user_calls()`. Add the registry generation to the
engine's plan-cache key.

## R3. Number-format rendering and cell presentation live in calc-func, and the apps call it directly

Evidence
- Plan §4.4: "CalcFml owns the formatting engine ... CalcFunc ... calls an injected lower-level
  `NumberTextService`". The code does the opposite. `calc-func/src/format/mod.rs` (1,364 lines)
  exports `cell_text` (`:1052`), `cell_color` (`:1126`), `DEFAULT_PALETTE` (`:1095`) and
  `general` (`lib.rs:106-110`). The calc-fml crate doc (`calc-fml/src/lib.rs:1`) says it holds
  "number formats", but it only re-exports them.
- `calc-func/src/call.rs:218-286`: `CellFormatFacts`, `HorizontalAlignment`, `ColumnWidth`,
  `WorkbookFile`, `ApplicationFacts` (INFO's OS and release) are host and presentation types in the
  function crate. They are re-exported upward (`calc-fml/src/lib.rs:52-56`) and filled by calc-host
  (`calc-host/src/facts.rs:100-204`).
- The apps skip the host and call calc-func for display: `apps/dna-calc/Cargo.toml` depends on
  calc-func for `cell_text`. The calls are at `apps/dna-calc/src/cells.rs:187,209`,
  `workbook.rs:232`, and `apps/dna-formula-ui/src/result.rs:184,428`.
  `result.rs:430` hard-codes `DateSystem::System1900` instead of the session's environment.
Why it is weak: grid, CF number formats, formula UI and TEXT share a renderer only because all
reach into calc-func; no shared format context (date system, locale, palette) is passed.
What builds on it: dc-971p.6 (grid formats, alignment, fonts: "one existing formatting route and
actual column width"), dc-zsn.4 (locale month names), dc-jnt.12 (number-format tests), dc-wr9
(OneCalc format panel), precision as displayed (`calc-host/src/state.rs:47-65`, no bead yet).
Direction: either move the renderer to calc-fml as the plan says and inject a narrow
`NumberText` service into calc-func for TEXT/FIXED/DOLLAR/VALUE, or amend the plan and stop
claiming it. In either case, apps should get display text through one host or fml presentation
function that takes the same format context the engine uses.

## R4. The same presentation fact is derived by two routes (grid vs CELL, style lookup, column width)

Evidence
- Cell style index: `calc-host/src/state.rs:649-657` (`HostState::cell_style`) and
  `calc-host/src/facts.rs:86-97` (`HostFacts::style`) are copies. Each maps positions through its
  own axis-edit helper (`facts.rs:37` vs `self.opened_position`).
- Column width: CELL("width") uses `facts.rs:53-71` `maximum_digit_width`, which refuses any font
  other than Calibri or Aptos Narrow 11. The grid hard-codes `DIGIT_PX = 7.0`
  (`apps/dna-calc/src/grid/mod.rs:65`) and estimates text width as `chars * 7 + 5`
  (`grid/render_plan.rs:140-143`) for whatever font the workbook has.
- Alignment: CELL("prefix") reads the file's `<alignment horizontal>` through
  `facts.rs:114-127`. The grid places text by value kind (`render_plan.rs:154-172`) and reads
  alignment only to refuse AutoFit (`apps/dna-calc/src/server.rs:661-668`).
- Sheet name lookup: `calc-engine/src/read.rs:197-211` (`ReferenceCatalog`, used by references
  and INDIRECT) folds with Unicode `to_uppercase`. `read.rs:1096-1108` (`HostFacts::sheet_named`,
  used by SHEET("name")) folds ASCII only. A non-ASCII sheet name resolves for INDIRECT but not
  for SHEET.
Why it is weak: each pair agrees only on measured fixtures; dc-971p.6 will build on the app route.
Direction: one host-level "resolved cell presentation" query (style index → number format,
alignment, font metrics → column width in characters and pixels). The grid and `WorkbookFacts`
both consume it. Use one sheet-name matcher in calc-fml, the same one the binder uses.

## R5. Locale is half explicit context and half ambient en-ZA

Evidence
- Collation is fixed to en-ZA (`calc-func/src/lib.rs:44-50` `text_order`;
  `catalogue/comparison.rs:22`, `comparison/sort_key.rs:3`) with no environment parameter.
  DNA Calc opens every workbook as en-US (`apps/dna-calc/src/workbook.rs:28,37`).
- `LocaleId::CurrentExcelHost` is the owner's machine: rand currency, space as group separator
  (`locale.rs:968-985`), lowercase am/pm shared with en-ZA (`format/mod.rs:49-53`).
- The clock is a value inside `CalcEnvironment` (`locale.rs:1111-1115`), not a service like
  `RandomSource` (`call.rs:78-81`). OneCalc samples it once when the environment is set
  (`apps/dna-onecalc/src/surface.rs:153-158`). DNA Calc never calls `set_clock`, so NOW and TODAY
  are refused there.
Why it is weak: one workbook compares text under en-ZA but parses and formats under en-US.
What builds on it: dc-zsn (collation, Unicode folding, locale-aware parsing), dc-zsn.4,
dc-jnt.14, dc-wr9 (locale choice).
Direction: a `LocaleContext` that carries the format profile and the collation together, held
in the evaluation context, with `CurrentExcelHost` resolved at the app boundary. Make the clock a
callback service next to random.

## R6. Changing the clock invalidates the whole engine

Evidence: `calc-host/src/state.rs:234-238` `set_clock` → `engine.set_environment`. In
`calc-engine/src/state.rs:783-789`, a changed environment makes every formula node pending.
Volatile NOW/TODAY are already recalculated every time (`recalc.rs:515`).
Why it is weak: once DNA Calc sets the clock before each recalculation (needed for NOW/TODAY),
every recalculation becomes a full one.
Direction: split environment into a semantic part (changing it makes nodes pending) and volatile
services (clock, random) that are read per tick and never dirty anything.

## R7. Conditional formats and data validation live in the host, and their model form needs the codec to read

Evidence: `calc-host/src/conditional.rs:1-10` evaluates CF through `DetachedFormula` against
published results. `:703-713` re-parses each rule with `calc_doc::conditional_rule(source)` on
every `conditional_formats` call. The model keeps the rule as raw XML bytes
(`calc-doc-model/src/conditional.rs:6-15`). Plan §6.4 says CF and DV are engine participants with
dependencies. `tools/xtask/architecture.json` forbids calc-engine → calc-doc, so the engine can
never interpret these rules as they are stored.
The app also reads codec load records directly for live state: protection, views,
unread CF containers and validations (`apps/dna-calc/src/server.rs:603,713,827,887,1365,1627`
via `calc_host::LoadedSheet`, re-exported at `calc-host/src/lib.rs:96`). These are load-time
facts, and edits never update them.
What builds on it: dc-jnt.17 (lands in `calc-host/conditional.rs`), dc-971p.6, pivots (dc-8wp).
Direction: parse rules into a typed model record at load (keeping source bytes for round-trip),
so CF can become an engine participant and the app reads session state, not load records.

## R8. Function identity and file spelling are matched by string in several layers

Evidence
- Volatility is a name list in calc-fml (`eval/mod.rs:190-201`), not catalogue metadata (plan
  §4.3: "the catalogue declares ... volatility once").
- The engine detects SUBTOTAL/AGGREGATE by spelling, including `"_XLFN.AGGREGATE"`
  (`calc-engine/src/recalc.rs:1161-1166`).
- There are two file-prefix tables: calc-func `file_prefix.rs` (`FilePrefix`, used by calc-fml
  `compile.rs:2233`), and calc-doc's own `Prefix` enum plus 509 rows in
  `save/file_form_table.rs` (3,571 lines, `file_form.rs:44-60`). A third substring check
  is in `calc-engine/src/read.rs:1074` (FORMULATEXT refuses `_xlfn.`).
Why it is weak: a new function needs edits in up to four places tied only by separate tests.
Direction: volatility and the "is a subtotal" flag as catalogue metadata, engine decisions by
`FunctionId`, and one prefix table (calc-func's) that calc-doc reads through calc-fml.

## R9 (minor). Edges and globals

calc-doc (codec) → calc-fml → calc-func pulls the 77k-line function crate into the codec for the
lexer (`load/shift.rs:30`); `calc-doc-model/src/root.rs:18` has a process-global `NEXT_LINEAGE`
atomic; `calc-vba` falls under no `architecture.json` rule; `help.rs` (4,516 lines of UI text)
is in calc-func.

## What each boundary's explicit context would need (inferred from what is passed by side routes today)

- **func ← fml (`EvalServices`)**: numeric mode; locale (format profile and collation); date
  system; clock and random callbacks; caller cell; cell reader (values, subtotal exclusions);
  reference-text resolver; callable invoker; host facts as abstract questions. CELL's needs
  (prefix char, width in characters, format class, locked, file name) come as answers, not style
  enums. One route for each, no fallback.
- **fml ← engine/host (`FormulaEvaluationContext`)**: the above, plus the reference catalogue,
  the UDF resolver with generation and call callback (thread affinity declared), observer and
  effect sinks, and the format context shared with the presentation query.
- **engine ← host**: semantic environment (makes nodes pending) apart from per-tick volatile
  services; the workbook-facts snapshot; UDF registry with generation; thread policy; CF and DV
  rules as typed model records.
- **host ← app/worker**: clock source, application facts (INFO), file identity, locale choice
  (resolving "current host"), threads, registered add-ins. In return, a presentation query
  (shown text, colour, alignment, width), so the app stops calling `calc_func::cell_text` and
  reading `LoadedSheet` directly.
