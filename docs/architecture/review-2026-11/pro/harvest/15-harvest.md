# Harvest 15: dependency graph and recalculation (GPT-6 Pro answer, read against 3b5aa86b)

Reader: Opus, 2026-11-12, for dc-6332. Answer: `pro/15-dependency-graph-and-recalc-answer.md` (758 lines);
prompt: `pro/15-dependency-graph-and-recalc-prompt.md` (1,257 lines). Both read in full.

Pins: DnaCalc 3b5aa86b, CalcVBA 8115aa78 (the answer makes no CalcVBA claim, so the CalcVBA checkout was not
needed). **Pinned code is current code for this topic.** `git diff --stat 3b5aa86b HEAD` is empty over
`crates/calc-engine/src`, `calc-host/src`, `calc-fml/src`, `calc-xll-host`, `calc-xll-abi` and
`calc-catalogue/src/ported.rs` (the 151 commits since the pin touch calc-func, two calc-catalogue admission files,
calc-doc load, tests and docs). The working tree of those crates is clean, so every `file:line` below holds at both
3b5aa86b and HEAD (eccea1ab).

How the checks were done: reading the code named below; `.beads/issues.jsonl` read with a Python one-liner (no `br`
writes); `docs/architecture/VOLATILITY.md`, `EVALUATOR_SUSPENSION.md`, `RECALC_CONTRACT.md` and `EXCEL_CALC_PROBES.md`
read for what is already measured; the probe fixture folders listed. Not done: no cargo build or test, no Excel run,
no tracker write. Inferences from code that a test would settle are marked "(code, not run)".

**What the answer is.** It read only the prompt (line 5). The prompt gave it 20 excerpts, but not
EXCEL_CALC_PROBES.md, the iteration code or the tracker. So the answer treats several things as open that are already
built or measured: requirement versions, async token validation, iteration order and seeds, Range.Calculate
post-state, cached VBA reads and no value cutoff. Its design advice is sound and mostly agrees with our contract.
Its new value is in a few experiments (P16, P12, the bare-reference quirk, convergence, fact invalidation) and in
naming the volatility and scaling gaps. The answer uses "gate" for acceptance tests; this harvest says "acceptance".

---

## 1. Findings that change how the answer should be used

1. **Five of the answer's ranked risks are already handled in code (R2, R7, R11, R6, R1).** The answer could not
   see the code, so read its section 2 as a checklist, not a defect list.
   - R2 (an old candidate satisfies a newer invalidation): `RequirementState` exists (`calc-engine/src/contract.rs:187-191`).
     Every invalidation advances `required` to a new epoch (`:586-601`). `satisfy` refuses a stamp certified for another
     `required` (`:632-636`). `RECALC_CONTRACT.md:497-499` ("Today a node is pending or not") is stale.
   - R7 (stale async completion): `AsyncToken` carries invocation, formula generation, requirement, provider and
     request (`contract.rs:66-72`, `:130-139`). It has typed rejections `TargetChanged`, `RequirementsChanged`,
     `ArgumentsChanged`, `Revoked` and `ConflictingDuplicate` (`:141-152`). Excel's side was measured in P07.
   - R11 (removing a runtime edge removes a structural one): `NodeSet` is a set, not a count (`graph.rs:710-760`), but
     the runtime overlay never holds an area a placed reference holds (`recalc.rs:6303-6334`). Every rebind clears both
     (`state.rs:1617-1629`, `:1300-1314`). No path found where they overlap.
   - R6 (RTD update during a first, unpublished reader): an accepted update advances `contract.external_inputs`
     (`state.rs:714-718`). The running work is then not `inputs_current` (`recalc.rs:690-698`) and is superseded, with
     staged results kept only where their certificate holds (`calc-host/src/recalc.rs:219-240`). (code, not run)
   - R1 (result and runtime footprint installed apart): the staged overlay is installed in the same publication call
     as the result (`recalc.rs:2736-2746`, `:2951-2953`). It is dropped for excluded range targets (`:2242`).
2. **The volatility fix is smaller than the answer says, but the code has two more errors it did not name.**
   - The sticky OR is at four sites, not one: `recalc.rs:2730`, `:2918`, `:4878`, `:5696-5698`. Confirmed defect (D1).
   - "No request" is already recorded per evaluation. A plan with a user call always gets
     `udf_volatility = Some(...)` (`recalc.rs:5508-5510`). `UdfVolatility::NoRequest` is distinct from `Set(false)`
     (`calc-fml/src/eval/mod.rs:409-421`). XLL `!` and `#` defaults already enter the same ordered channel at call
     start (`calc-xll-host/src/invoke.rs:152-156`, overwrite at `calc-fml/src/eval/run.rs:4287-4291`).
     So F2 for user-function requests is one decision at the four sites: `Set(b) => b`, `NoRequest => S`.
   - **New: name bodies are folded into the cell's token S.** The compiler lowers a name body into a block of the
     caller's own plan (`calc-fml/src/eval/compile.rs:1358-1359`, `:1901-1903`). `CompiledPlan::volatile` scans every
     block (`mod.rs:364-366`). Excel gives a cell no token from a name; executing the name is an event (VOLATILITY §5).
     Our code makes `IF(TRUE,1,NmRand)`, `IF(FALSE,NmRand,1)` and `NmRand+VFalse()` volatile. Excel measured all three
     nonvolatile. (code, not run)
   - **New: FORMULATEXT, SHEET and SHEETS carry no token.** `calc-catalogue/src/ported.rs:334-336` has no
     `volatile true`; CELL and INFO at `:332-333` do. VOLATILITY P1 measured all five as tokens.
   - Still missing as events: SUMIF/AVERAGEIF with a smaller sum range (no engine or catalogue code), a blocked spill
     (its would-be area is a runtime edge for invalidation only, `state.rs:47-52`, `:1080-1082`), and dropping requests in lifted calls (X2; `run.rs:4287-4291`
     always overwrites). The `#` default is static (`calc-xll-abi/src/signature.rs:169`): that is dc-6332.14.
3. **The answer asks for timing baselines that exist, and its budgets are far from them.** dc-hsa5.13 and dc-hsa5.17
   measured 1,000,002-formula books: rebuild 10-18 s, peak RSS 2.97-3.63 GiB, a 3-cell edit 1.51 s. dc-10d9 measured
   the DTC one-cell edit at 0.896 s against Excel's 0.222 s. The answer's targets (512 MiB, 50 ms p95) are 6x and 30x
   below today. They are invented targets, and the owner put performance after publish. Defer them.
4. **There is one real scaling hazard: shared ranges of formula cells expand quadratically.** `closure_with` stores
   each member's full reader list (`recalc.rs:3454-3503`). A range of M formula producers read by N formulas gives
   M x N closure edges. The common share-of-total layout (B[r] a formula, C[r] = `B[r]/SUM(B$2:B$30001)`) gives
   9 x 10^8 edges at 30k rows. The `u32` `expect` at `:3500-3501` ends in a panic at 2^32. Whole-row and whole-column
   edges are scanned linearly for every point query (`graph.rs:553-560`), so N formulas on `A:A` cost N per query.
   The answer's R9 inferred this correctly. Nobody has measured it. It may explain dc-n2st (one workbook over 600 s)
   or the 19 Enron timeouts. Bead 15.c.
5. **About half of the answer's experiments are already measured.** E3 (no value cutoff: EXCEL_CALC_PROBES.md §1
   row 1), E5 (P02), E6 (the suspension study, and X4 for request survival), E9 seeds and sweep order (dc-86f,
   dc-10y, P14 eng9-eng17), E11 (x17 regression, 28 rows), E13's Excel half (P07) and E15's VBA half (P01) are done.
   The new ones worth running are P16 (E1/E2), P12 shadows (E4), the bare-reference quirk (E8), the stopping rule
   (E9), fact and 3-D invalidation (E10) and spill-block order (E7). Most need F2 in DNA before DNA can be compared.
6. **The restart question (D2) is a known open bead, and the owner's bit-match rule decides one half of it.**
   dc-hsa5.20.10 says values never differ and only effect counts do. It asks whether to "model or document".
   Bit-match-always rules out "document". The answer adds one point the bead lacks. Excel's restart depends on its
   initial calc order: it shows only with a three-level chain on the first calculation, never one level. So changing
   the wait into a reevaluation is not enough; the engine must reproduce which reads find an uncalculated producer.

---

## 2. Disposition of every section

| Section | Content | Disposition |
|---|---|---|
| 1 (opening) | Keep engine and graph; fix volatility and restart before performance | no action: agrees with dc-hsa5.20 (keep architecture) |
| 1.1 | What to keep: placed/runtime split, block index, host command seams, status 64, RTD | no action: all confirmed (section 3, C4-C9) |
| 1.2 volatility | Recompute per completed evaluation | **15.a**; events part **15.b**; `#` part **dc-6332.14** |
| 1.2 suspension | Documented divergence | **dc-hsa5.20.10** (finding 6) |
| 1.2 one graph, several questions | S / observations / waits / cached reads kept apart | no action: already true (`RECALC_CONTRACT.md:501-509`; contract.rs `ReadDetails.observations` apart from reads) |
| 1.3 | Route table: edit, UDF, RTD, async, VBA/JS/skin, save/refresh | no action: summary. JS/skin rows belong to harvests 14 and 17; save to 16 |
| 1.4 | Population table | no action: repeats the prompt correctly |
| 2 D1 | Sticky OR | **15.a** (four sites) |
| 2 D2 | Retained progress vs Excel restart | **dc-hsa5.20.10** |
| 2 R1 | Result and footprint installed apart | no action: same publication call (finding 1) |
| 2 R2 | Second invalidation | no action: implemented; fix the stale doc line in **15.a** |
| 2 R3 | Old dynamic cycles; names/spans omitted from fallback | no action: fallback confirmed (`recalc.rs:3576-3608`); name-body references are placed references of the caller (compile.rs:1358), so nothing is omitted. E9 topology arm goes to **dc-hsa5.14** |
| 2 R4 | Name shadow/miss rebind | **15.d** (X2) then **dc-hsa5.6** |
| 2 R5 | Spill footprint changes | no action: before-and-after union (`recalc.rs:4954-4960`), P05 measured. 3-D/fact arms in **15.d** |
| 2 R6 | RTD first unpublished reader | no action: whole-work input cut (finding 1). Batch semantics **dc-snz.23** |
| 2 R7 | Stale async | no action: typed token validation exists (finding 1) |
| 2 R8 | Range completion vs global freshness; same-revision deltas | Range part: no action (P02, `calc-host/src/recalc.rs:164-171`). Transport part: harvest 17 |
| 2 R9 | Closure expansion, `u32` bounds | **15.c** |
| 2 R10 | Global reader-cache epoch | deferred (performance after publish; every `file()` bumps the epoch, `graph.rs:459`) |
| 2 R11 | Runtime removal erases structural edge | no action: excluded by construction (finding 1) |
| 2 R12 | Span type vs 3-D product support | no action: comment is accurate; 3-D refs expand per sheet at compile (`compile.rs:2024-2064`). Membership arm in **15.d** (X6) |
| 2 R13 | Save/JS/skin adapters bypass authority | no action here: harvests 14, 16, 17 |
| 2 closing paragraph | Old RTD and status-64 defects are fixed | no action: agrees |
| 3 intro | Extend existing types, no parallel API | no action: owner rule already |
| 3.1 | Owner table across the five reviews | no action here: harvest 16 (engine-host-doc) owns the cross-review table |
| 3.1 identities | Generation where an address stands in | no action: formula generations exist (`contract.rs:563-569`, `EvaluationTarget.formula_generation`) |
| 3.2 S/R/D/O | Four dependency kinds | no action: matches `ReadDetails` (`contract.rs:270-292`) and the read table in RECALC_CONTRACT §4 |
| 3.2 subject table | Per-subject binding/runtime needs | Name row **dc-hsa5.6**; table/3-D/fact rows **15.d** (X6) first; rest no action |
| 3.2 Rust sketch | `EvaluationEvidence`, `EvaluationStop` | **dc-hsa5.20.3** (typed stop reasons, evidence tape) already covers it |
| 3.3 | Names: per-use evaluation, miss proofs, no name value cache | Per-use and no cache: no action (state.rs:1461-1462). Volatility of names: **15.a**. Miss proofs: **15.d** X2 then **dc-hsa5.6** |
| 3.4 | Eight certificate invariants | no action: 1, 2, 4-6 implemented (contract.rs:586-636; RECALC_CONTRACT §4). 7 (force) not checked here |
| 3.5 | Attempt dispositions table; S/D ownership | no action: D excludes S (`recalc.rs:6303-6334`), staged overlay dropped on exclusion |
| 3.6 | Atomic publication, spill arbitration, blocked-spill volatility | Publication: no action. Blocked-spill event: **15.b** after X3 |
| 3.7 | Volatility reduction table | **15.a** (S, requests, names, tokens); **15.b** (SUMIF, spill, lifted); **dc-6332.14** (`#`) |
| 3.7 `ca` and dependents | Save projection of `ca`; bare-reference exception | `ca` on load: **dc-kapl.2.1**. Bare-reference rule: **15.d** X4, then deferred |
| 3.8 command table | F9, Shift+F9, Range, full, rebuild, cancel | no action: implemented and P02 measured (`calc-host/src/recalc.rs:146-185`, `state.rs:1300-1322`) |
| 3.8 learned order | calcChain as hint; early entry | **dc-hsa5.20.10** (finding 6) |
| 3.8 completion | Qualified completion receipts | no action: `CompletionReceipt`, `CompletionOutcome` exist (session.rs:580-586) |
| 3.9 | Cycles and iteration | Order, seeds, in-place sweep, stop rule implemented (`recalc.rs:36-67`). Open profiles **dc-hsa5.14**; iteration off **dc-y2o.1**. Stop-rule arm X5 in **15.d** |
| 3.10 RTD trace | Batch, lease, initial adoption | **dc-snz.23**, **dc-hul4** (clock RTD; owner 2026-11-12) |
| 3.10 async trace | Two-stage validation | no action: built (finding 1) |
| 3.10 status-64 trace | x17 sequence | no action: matches XLL_COM_HOSTING §30 and the code |
| 3.10 refresh trace | One provider route first | deferred: no refresh provider yet (section 7) |
| 3.11 | Off/Safe/Unguarded, native trust, wasm, OneCalc | no action: agrees with owner rules |
| 3.12 | Structural changes before tuning | **15.c** for the quadratic case; rest deferred |
| 4 intro | Reuse fixtures and verdict path | no action |
| Stage A | Trace correlation, route classification | no action: a route ledger is process with nothing depending on it (owner rule); receipts already exist |
| Stage B | Completed-evaluation volatility | **15.a**, **15.b** |
| Stage C | Single install transaction, S/D ownership, first unpublished readers | no action: built (finding 1) |
| Stage D | Entry order, scopes, cycles | **dc-hsa5.20.10**, **dc-hsa5.14** |
| Stage E | RTD batch/lease, async tokens, refresh, JS/skin/save | RTD **dc-snz.23**; async no action; refresh deferred; JS/skin/save harvests 14, 16, 17 |
| Stage F | Scaling | **15.c** for the hazard; rest deferred |
| Budgets table | 512 MiB, 50 ms, linear growth | deferred: performance after publish; baselines exist (finding 3) |
| 5 common method | Raw bits, entry logs, warm/cold rules, VBA helpers | no action: the probe route already does this (EXCEL_CALC_PROBES.md §0). Use its `Tk`/`Cnt` helpers, not new ones |
| E1 | Untaken branch vs executed read; circular control | **15.d** (P16 arm; X1). Circular control partly answered: `=IF(TRUE,"x",F5)` in F5 is calculated (recalc.rs:41) |
| E2 | Dynamic-edge retirement | **15.d** (X1; this is P16, never run) |
| E3 | Equal-bits propagation | no action: measured (EXCEL_CALC_PROBES.md §1 row 1: re-invoked by a dirty upstream with unchanged result) |
| E4 | Name shadows, relative names, stateful names | **15.d** (X2); P12 has only three captures (eng19) |
| E5 | Range poststate, boundary caches | no action: P02 measured (StillRequired, boundary caches) |
| E6 | Cold-chain restart | no action as an experiment: suspension study + X4. Decision: **dc-hsa5.20.10** |
| E7 table rows | F1/F2/F3 | no action: measured (P2) |
| E7 events, contexts, lifting | `!`, `#`, SUMIF, names, callback statuses, X2 | no action: measured (P3, P4, P5, P6, X2, X3) |
| E7 spill order | Blocked spill vs request order | **15.d** (X3); VOLATILITY §8 lists it open |
| E7 completed error | Request kept on error completion | **15.d** (X3) |
| E8 | Bare-reference dependent quirk | **15.d** (X4). The packet omitted VOLATILITY.md:20-22, which gives the shape (`=Cnt(A1)`); Matrix B fits it |
| E9 seeds and order | One-sweep and seed tables | no action: measured (dc-86f, dc-10y, P14) |
| E9 convergence | Max vs sum, absolute vs relative | **15.d** (X5), results to **dc-hsa5.14** |
| E9 topology | Acyclic-cyclic-acyclic retarget | **dc-hsa5.14** (its "cycle membership changing" item); arms in X5 |
| E10 spill | Growth, shrink, blockers | no action: P05 measured |
| E10 pending blank | Reader before spilling producer | no action: P05 reader-first measured |
| E10 3-D, table, CELL format, sheet facts | Membership, table rename, fact invalidation | **15.d** (X6), after 15.a (the arms need F2 in DNA) |
| E11 | Status 64 vs `#` and value arguments | no action: x17 regression already in tests |
| E12 | RTD initial, batch, equal payload, lease | **dc-snz.23**; equal-payload and throttle arms to **dc-hul4** |
| E13 | Async stale authority | Excel side measured (P07); native XLL async **dc-6332.13** (step 5) |
| E14 | External links, materialised refresh, data tables | deferred (section 7) |
| E15 VBA | Cached observation | no action: P01 measured |
| E15 Office.js | JS API read/calculate contract | deferred to harvest 14 (object model and JS) |
| Build controls | Reproduce 20527 first | no action: our captures all name 16.0.20527 |
| 6 sticky OR | Delete after reducer | **15.a** |
| 6 retained progress | Stop treating it as universal | **dc-hsa5.20.10** |
| 6 duplicate adapters | Prohibition | no action: owner rule already |
| 6 one boolean, many meanings | Typed outcomes | **dc-hsa5.20.3** |
| 6 cutoff and blanket restart | Stop both | no action: neither exists (RECALC_CONTRACT decision 10; RTD per topic) |
| 6 cached reads vs edges | Keep edges | no action: agrees |
| 6 stale prose | Pending-only sentence, span comment | RECALC_CONTRACT.md:497-499 is stale: **15.a** fixes it. Span comment (state.rs:1409-1413) is accurate: no action |
| 6 adjacency copies first | Before exotic infrastructure | **15.c** |
| 6 defer claims | Browser parity, WebGPU, refresh | no action: agrees |
| 6 keep artefacts | Preserve captures | no action: agrees |
| Locators | Link list | no action |

---

## 3. Claims checked against the code

| # | Claim (answer section) | Verdict | Evidence at 3b5aa86b |
|---|---|---|---|
| C1 | Publication ORs the UDF request with the plan flag (D1, 1.2) | confirmed, four sites | `recalc.rs:2724-2735`, `:2914-2922`, `:4876-4878`, `:5694-5699` |
| C2 | Replacing the OR with the last UDF boolean is not enough; no-request evaluations must publish too (1.2, 6) | partly | No-request is recorded: `recalc.rs:5508-5510`, `mod.rs:409-421`. A plan without user calls takes S at binding (`recalc.rs:3117-3122`), right under F2 when nothing else fires. What is still missing: names in S, three tokens, SUMIF/spill/name events, the lifted-call drop, the `#` rule |
| C3 | Plan flag means "built-in volatile"; token covers untaken branches (3.7) | confirmed | `mod.rs:360-366` counts calls in lazy branches; test `run.rs:3020-3077` |
| C4 | Placed vs runtime prerequisites; `set_dynamic` returns early on equal sets (1.1) | confirmed | `state.rs:21-26`, `:1390-1405` |
| C5 | Block index (≤4 blocks per rectangle), axis and span indexes, namespace proofs (1.1) | confirmed | `graph.rs:349-368`, `:458-525`, `:385-410` |
| C6 | Index complexity not established (1.1) | confirmed, and worse for axes | Axis edges of a sheet are filtered linearly per point query (`graph.rs:553-560`); spans likewise (`:561-566`) |
| C7 | Host: Manual returns early; except-tables deferral; Manual forced range = StillRequired (1.1) | confirmed | `calc-host/src/recalc.rs:148-171`, `:197-204`; deferral filters only data-table cells (`recalc.rs:3265-3267`) |
| C8 | Status-64 latch, discarded return, Reevaluate route (1.1) | confirmed | `callbacks.rs:323-327`; `invoke.rs:303-325`; `recalc.rs:2162-2189` |
| C9 | RTD invalidation filters by published stamps and advances external versions (1.1) | confirmed | `state.rs:675-718`; readers filed at publish, `state.rs:496-502` |
| C10 | Initial ConnectData adoption is narrow (3.10) | confirmed | `recalc.rs:669-680` fails with `ExternalInputMismatch` on a cut mismatch; caller `calc-host/src/recalc.rs:219-229` |
| C11 | R1: result and footprint may be installed apart | wrong | Same call: `recalc.rs:2736-2746`, `:2951-2953`; dropped for excluded targets `:2242`. Partial: a published NotModeled result installs its partial footprint (code, not run) |
| C12 | R2: a second invalidation may be satisfied by the old result | wrong (handled) | `contract.rs:586-601`, `:632-636` |
| C13 | R3: cycle fallback keeps placed edges; may omit names/spans | partly | Fallback confirmed `recalc.rs:3576-3608`. Name-body references are the caller's placed references (`compile.rs:1358`, `:1901-1916`); `Prerequisite::Name` is for rebinding only (`graph.rs:127`). Spans never produced (C20) |
| C14 | R4: namespace probes and refused-compile recovery exist | confirmed (machinery) | `graph.rs:385-410`; `state.rs:38-42`. Excel behaviour for add/delete shadow unmeasured |
| C15 | R5: spill tail/blocker risks | partly | Readers of the before ∪ after areas dirtied `recalc.rs:4954-4960`; a blocked spill keeps its would-be area as a runtime edge (`state.rs:47-52`, `:1080-1082`); `spill_epoch` `recalc.rs:2757-2760`. P05 measured |
| C16 | R6: first unpublished RTD reader missed | not a defect (code, not run) | `state.rs:714-718`, `recalc.rs:690-698`, `calc-host/src/recalc.rs:219-240` |
| C17 | R7: async completion may hit the wrong generation | wrong (handled) | `contract.rs:66-72`, `:130-152` |
| C18 | R9: closure appends reader incidences; `u32` hard bounds | confirmed | `recalc.rs:3454-3503` (`expect` at `:3500-3501`); `readers_into` returns every reader (`:3250-3270`). Quadratic size for shared ranges is inference |
| C19 | R10: global epoch defeats the readers cache | confirmed mechanism | `graph.rs:361-363`, `:459`; cost unmeasured |
| C20 | R11: removing D erases equivalent S | wrong | `recalc.rs:6303-6334`; `state.rs:1617-1629`, `:1300-1314`; `graph.rs:710-760` |
| C21 | R12: no product SheetSpan caller | confirmed, comment accurate | `recalc.rs:3175-3203`; `compile.rs:2024-2064`; structural sheet edits unbind moved nodes `edit.rs:706-725`, `state.rs:1645-1662` |
| C22 | Cycle members by folded sheet name, row, column (3.9) | confirmed | `recalc.rs:3689-3718`; dc-86f closed |
| C23 | Seeds, sweep visibility and stopping are open (E9) | wrong for this build | `recalc.rs:50-67`: in-place sweep in cell order, seeds from held values, stop when largest change < MaxChange; dc-hsa5.11 closed; P14 fixtures eng9-eng17 |
| C24 | Iteration off: reproduce held caches (3.9) | confirmed, built | `recalc.rs:36-49`; dc-y2o.1 rule |
| C25 | No value cutoff (3.4, 6) | confirmed | RECALC_CONTRACT decision 10; Excel: EXCEL_CALC_PROBES.md §1 row 1 |
| C26 | Range.Calculate leaves the target required (E5) | confirmed, measured | P02 measured 2026-11-01; code `calc-host/src/recalc.rs:164-171` |
| C27 | Cached VBA read is an observation, not an edge (E15) | confirmed, measured | P01 measured; `RECALC_CONTRACT.md:505` |
| C28 | Names have no shared value (3.3) | confirmed | `state.rs:1459-1462` |
| C29 | Generations should replace address identity (3.1) | already true | `contract.rs:563-569`; `EvaluationTarget.formula_generation` |
| C30 | No baseline timings or memory for this pin (4) | true of the packet only | dc-hsa5.13, dc-hsa5.17 (1M formulas: rebuild 10-18 s, RSS 2.97-3.63 GiB, 3-cell edit 1.51 s); dc-10d9 (DTC edit 0.896 s vs Excel 0.222 s) |
| C31 | Token set includes FORMULATEXT, SHEET, SHEETS (3.7, from VOLATILITY) | code disagrees | `calc-catalogue/src/ported.rs:334-336` (no `volatile true`) vs `:332-333` |
| C32 | A cell gets no token from a name; name use is an event (3.3, 3.7) | code disagrees (code, not run) | `compile.rs:1358-1359`, `:1901-1903`; `mod.rs:364-366` |
| C33 | `#` default only when a reference is actually passed (3.7) | code disagrees | `signature.rs:169`, `invoke.rs:152-156`; owned by dc-6332.14 |
| C34 | Lifted multi-cell UDF calls honour no request (3.7, X2) | code disagrees (code, not run) | No lifting check on the request path: `run.rs:4287-4291` |
| C35 | SUMIF shape and blocked spill are events (3.7) | code lacks both | No SUMIF volatility in calc-engine or calc-catalogue (dc-kapl.2.5 notes SUMIF never volatile); a blocked spill is only an invalidation edge (`state.rs:47-52`, `:1080-1082`) |
| C36 | The bare-reference exception's fixture is not in the packet (E8) | true of the packet | VOLATILITY.md:20-22: `=Cnt(A1)` not recalculated when A1 is volatile; `Cnt(A1&"")` normal |
| C37 | X4: abandoned attempt's request lost (E6) | confirmed, measured | VOLATILITY.md:128-130 (prompt line 353-356) |
| C38 | Suspension retains progress (D2) | confirmed | `EVALUATOR_SUSPENSION.md:60-87`; dc-hsa5.18 comment "DNA keeps resume-in-place"; dc-hsa5.20.10 open |
| C39 | Rebuild discards derived edges and reader-cache epoch (3.8) | confirmed | `graph.rs:371-379`; `state.rs:1300-1322` |
| C40 | Volatile `ca` dependents and persistence (3.7) | partly checkable | `ca` read on load but dropped after: dc-kapl.2.1 (its own description; not re-read here). Save `ca` from the dirty set not checked |

Claims about Excel behaviour the answer makes are labelled hypotheses in the answer itself; the ones worth testing
are in section 4.

---

## 4. Experiments

All of these are workbook runs in owned Excel sessions on the Windows desktop (Manual mode, VBA logging UDFs,
`Range.Calculate`, saves, iteration settings). The answer service cannot run them: it answers function calls only and
sends iteration, VBA and event work to workbook captures. Each runs through the existing probe route
(`crates/calc-engine/tests/fixtures/excel-calc-probes/`), Excel 16.0.20527, three trials per arm as in the
volatility probes. Use the existing `Tk` and `Cnt` helpers. The answer's `MarkD` takes a Double, which makes its
argument a value read; keep the `Cnt(x&"")` form where a value read is wanted. All predictions are hypotheses.

| Exp | Question | Inputs | Predicted outcome per hypothesis | Runs |
|---|---|---|---|---|
| X1 (E1, E2 = P16) | Do untaken references and old dynamic targets still dirty a formula? | B1=11, C1=22, A1=`"B1"`. Arms: (a) `D1=Cnt(IF(A2,B1,C1)&"")`, A2=TRUE, edit C1; (b) `=IF(TRUE,1,C1)+Tk("i",0)`; (c) `LET(f,LAMBDA(x,C1+x),1)+Tk("l",0)`; (d) `=INDIRECT(A1)+VFalse()+Tk("d",0)`, retarget to C1, then edit B1, then C1; (e) same with error target and `IFERROR`; (f) `=B1+INDIRECT(A1)+VFalse()+Tk("o",0)`; (g) OFFSET and reference-valued INDEX forms of (d). Warm, then the listed edits, `Application.Calculate` after each | Structural edge: (a)-(c) run on the C1 edit. Executed reads only: they do not. Replace overlay: (d) does not run on the old B1 edit, runs on C1. Accumulate: runs on both. (f) must run on B1 either way | 9 arms x 3 = 27 |
| X1b (E1 circular) | Does an untaken self-reference make a cycle? | Iteration off: A1 `=IF(FALSE,A1,1)` vs `=IF(TRUE,A1,1)` | Potential edge: both circular. Executed path: first gives 1. recalc.rs:41 already says `=IF(TRUE,"x",F5)` in F5 is calculated, so expect "executed path" | 2 x 3 = 6 |
| X2 (E4 = P12) | Does adding or deleting a sheet-local name rebind users without editing them? | Workbook `Pick` = `Sheet1!$B$1`; D1 `=Tk("D",0)+Pick`. Add local `Sheet1!Pick` = C1, Calculate; delete it, Calculate; recreate, edit the shadowed definition. Missing-name repair: `=Tk("m",0)+Nm` with Nm undefined, then define Nm. Nested missing name. Relative name from two callers. `NmCount=NextName()` used as `NmCount+NmCount` and `NmCount` | Miss proofs: 11, 22, 11, and no D entry on the shadowed edit. Resolved-ID only: stays 11 until rebuild. Spelling-wide: correct values but an extra D entry. Stateful: per use 3 entries, per formula 2, per name 1 | 6 arms x 3 = 18 |
| X3 (E7 new) | Where does a blocked spill's event sit against a request? Does an error completion keep a request? | A2=99. A1 `=SEQUENCE(2)+VFalse()`, separately `=VFalse()+SEQUENCE(2)`, each blocked and unblocked. Z1=0: `=VTrue()+1/Z1` vs `=VTrue()+1` | Event at placement (after all requests): both blocked copies volatile. Event at the expression position: order decides. Error keeps request: `#DIV/0!` cell recalculates on clean F9; discards: it does not | 6 arms x 3 = 18 |
| X4 (E8) | What is the bare-reference quirk's rule? | Source A1 volatile: (i) VBA `VTrue` counter, (ii) `RAND()`, (iii) XLL `!`. Consumers: `=Cnt(A1)`, `=Cnt(A1&"")`, `=Cnt(A1:A2)`, `=Cnt(NmA1)` (name to A1), `=Cnt(+A1)`, `=A1`, `=SUM(A1)`, a Variant UDF that never reads its argument, a Double-parameter UDF | Known: `Cnt(A1)` not recalculated, `Cnt(A1&"")` recalculated (VOLATILITY §1). Reference-argument rule: every by-reference UDF argument skips (ranges, names too); value rule: only unread references skip; Double parameter coerces at entry, so it recalculates | 3 sources x 1 book x 3 = 9, plus 2 Variant/Double books x 3 = 6; total 15 |
| X5 (E9) | Is the stop rule the largest change or the sum? Absolute or relative? Does a retired dynamic cycle stop iterating? | Iteration on, MaxIterations 5, MaxChange 1. (a) A1 `=B1+0.75`, B1 `=A1`, seeds 0. (b) A1 `=B1+2`, B1 `=A1`, stored seeds 1,000,000. (c) A1 target text, D1=10, B1 `=INDIRECT(A1)+VFalse()+Cnt(1&"")`, C1 `=B1/2`; D1 → C1 → D1, iteration on; again with iteration off | (a) largest change: one sweep, 0.75/0.75; sum: five sweeps, 3.75/3.75. (b) absolute: five sweeps, 1,000,010; relative: one sweep, 1,000,002. (c) retired: entry counts match an acyclic control after the switch back; stale SCC: extra entries | 4 books x 3 = 12 |
| X6 (E10 new) | Do 3-D membership, table edits and host facts dirty a formula without its token? | `D!A1 =SUM(A:C!A1)+Tk("s",0)`, A/B/C A1 = 1/2/4: edit B, move B out, insert a sheet with 8 between. Table T[Amount] 10,20: `=SUM(T[Amount])+Tk("t",0)`, append 30, rename the column, delete and recreate T. `=CELL("format",A1)&IF(VFalse()=0,"","")`: change A1's format, F9, then Full. `=SHEET()` and `=CELL("filename")` with a later VFalse: rename or reorder the sheet, SaveAs | 3-D current membership: 8, then 5, then 13. Table identity: 60 after append, follows the rename. Fact event: CELL updates on F9. Full-only: old text until Full | 8 arms x 3 = 24 |

Total: 120 workbook runs. Results go to EXCEL_CALC_PROBES.md (P12, P14, P16 sections) and VOLATILITY.md §8.

Experiments not proposed because they are measured: E3 (§1 row 1), E5 (P02), E6 (suspension study, X4), E7's
F1/F2/F3 table and context rows (P2-P6, X2, X3), E9's seed and order tables (dc-86f, dc-10y), E10 spill rows (P05),
E11 (x17), E13 COM/UI (P07), E15 VBA (P01). E12 (RTD batch, equal payload, lease reuse; about 30 runs with a
deterministic local RTD server) belongs with dc-snz.23 and dc-hul4. E14 and E15 Office.js are deferred (section 7).

What DNA needs before comparing: X1 (d)-(g), X3, X5 (c) and X6 use a false request to remove the token. Under our
sticky OR they stay volatile in DNA, so they need 15.a first. X2 and X4 do not depend on it.

---

## 5. Proposed beads

### 15.a F2 volatility decision at publication, without name tokens, with the three missing tokens (P1)

- **What.** Apply Excel's measured rule (VOLATILITY.md §2, F2) for the events we already record.
  1. At the four sites (`calc-engine/src/recalc.rs:2724-2735`, `:2914-2922`, `:4876-4878`, `:5694-5699`), decide
     with one function: `Set(b) => b`, `NoRequest => S`, where S is the plan's token.
  2. Keep name bodies out of S. Name blocks are lowered into the caller's plan (`calc-fml/src/eval/compile.rs:1358`,
     `:1901-1903`); make `CompiledPlan::volatile` (`calc-fml/src/eval/mod.rs:364-366`) skip blocks reached only
     through `Instruction::Name`. Record a name use whose body has a token as an event that sets true. The
     evaluator then needs the effects record for plans with names (today only plans with user calls get one,
     `recalc.rs:5508-5510`). Keep a separate "reads the clock or random stream" fact for `volatile_inputs`
     (`recalc.rs:5694-5699`), which must stay true for a name body with RAND.
  3. Add `volatile true` to SHEET, SHEETS and FORMULATEXT (`calc-catalogue/src/ported.rs:334-336`).
  4. Docs: fix `RECALC_CONTRACT.md:497-499` (requirement versions exist: `contract.rs:187-191`, `:586-636`) and the
     line references in `VOLATILITY.md:157` (now 2730, 2918, 4878, 5696).
- **Acceptance.** Engine tests with host-fake UDFs (`VFalse`, `VTrue`) on the measured VOLATILITY rows:
  `RAND()+VFalse()` and `VFalse()+RAND()` nonvolatile; `IF(TRUE,RAND(),VFalse())` volatile; `=NmRand` and
  `NmRand*0` volatile; `IF(TRUE,1,NmRand)`, `IF(FALSE,NmRand,1)`, `NmRand+VFalse()` nonvolatile; `VFalse()+NmRand`
  volatile; `LET(f,NmLam,1)` volatile; `=SHEET()` volatile; a UDF that requests true once and then nothing ends
  nonvolatile. Observable: entry counts on a clean Calculate. `cargo xtask corpus --oracle` and the full bank give
  the same counts or the changed cells are listed.
- **Dependencies.** None. It takes the decision function out of dc-hsa5.20.2, which keeps the evidence and
  `RecalcTrigger` work; dc-6332.14 follows it.
- **Model.** Sonnet (well described); Opus reads the name-use event change in calc-fml. Linux host (Windows Rust rule).

### 15.b Run-time volatility events: smaller SUMIF sum range, blocked spill, lifted calls (P2)

- **What.** Three events VOLATILITY measured and our code lacks. (1) SUMIF and AVERAGEIF with a sum range smaller
  than the criteria range set true, decided per evaluation (P1 `p1d`-`p1f`); SUMIFS, COUNTIF(S), MAXIFS, MINIFS do not.
  (2) A blocked spill sets true (`p1f`); its order against requests comes from X3. (3) Requests made inside an implicitly
  lifted multi-cell call are dropped; 1x1 references and MAP/BYROW calls keep them (X2). Place the events in the
  same ordered channel as 15.a.
- **Acceptance.** Tests on the measured rows: `SUMIF(C1:C3,1,D1:D2)` volatile, `SUMIF(C1:C3,1,D1:E3)` not;
  `SUMIF(mismatch)+VFalse()` not, `VFalse()+SUMIF(mismatch)` volatile; the toggled `n` case 2, 3, 2; the X2 lifted
  rows. Corpus and bank counts as in 15.a.
- **Dependencies.** 15.a; the spill part waits on X3 (15.d).
- **Model.** Sonnet.

### 15.c Shared ranges of formula cells: measure the closure, then bound it (P2)

- **What.** `closure_with` keeps every member's reader list (`calc-engine/src/recalc.rs:3454-3503`), and whole-axis
  edges are scanned linearly per point query (`calc-engine/src/graph.rs:553-560`). Measure first, through the normal
  host and the existing bench route, on generated books: share-of-total (B[r] `=A[r]*2`, C[r] `=B[r]/SUM(B$1:B$N)`)
  and whole-column (C[r] `=B[r]/SUM(B:B)`, D[r] `=SUMIF(A:A,A[r],B:B)`), N = 10k, 30k, 100k. Record closure edge
  count, readers-cache size, peak RSS and time for open, full calculation and a one-cell edit. Also check whether the
  dc-n2st workbook (e54cd64b…) and the 19 Enron files that timed out at 120 s have this shape. If the expansion is
  confirmed, Opus designs a grouped representation (one area subject with producer membership and its readers) for
  closure and ordering that keeps the same order and the same entry counts; until then, a typed refusal before the
  `u32` limit or memory runs out, never a panic.
- **Acceptance.** A table of the measured sizes per N; after a fix, edge count grows with M + N, not M x N; corpus
  and bank counts unchanged; Ofgem stays 209,272 of 209,272.
- **Dependencies.** None for the measurement.
- **Model.** Sonnet for the measurement (Linux host); Opus for the grouped design; Codex terra for its implementation.

### 15.d One Excel batch for answer 15's open recalculation questions (P2)

- **What.** Run X1, X1b, X2, X3, X4, X5 and X6 of section 4 (120 workbook runs) through the existing
  `excel-calc-probes` route on the Windows desktop, three trials each. Record raw bits, `Tk`/`Cnt` entry logs,
  `ca` bits after SaveCopyAs, and the commands in order. Write the results into EXCEL_CALC_PROBES.md (P12, P14, P16)
  and VOLATILITY.md §8, with raw files under `crates/calc-engine/tests/fixtures/excel-calc-probes/`.
- **Acceptance.** Each arm has three agreeing trials or the disagreement recorded; every prediction in section 4 is
  marked confirmed or refuted; the beads they feed (15.b, dc-hsa5.6, dc-hsa5.14, dc-kapl.2.1) get the result.
- **Dependencies.** None (no Rust). DNA's comparison of X1 (d)-(g), X3, X5 (c) and X6 waits on 15.a.
- **Model.** Sonnet.

**Start first:** 15.a (the measured rule is settled and the defect touches every formula with a UDF or a volatile
name; it also releases dc-6332.14); 15.d (no Rust, so it can run on Windows now, and it feeds 15.b and two open
beads); the measurement half of 15.c (cheap, and it may explain dc-n2st and the Enron timeouts).

### Notes for existing beads (for the orchestrator; I did not write them)

- **dc-hsa5.20.10**: under bit-match-always, "keep it as a documented difference" is ruled out. The model must
  reproduce which reads meet an uncalculated producer on the first calculation (three-level chains only, measured),
  so the mode is about initial order, not only the wait instruction. An abandoned attempt's requests are dropped (X4).
- **dc-hsa5.20.2**: its line references (2613-2619, 5343-5346) are stale; 15.a takes the decision function.
- **dc-hsa5.14**: add X5's stop-rule and retarget arms.
- **dc-hsa5.6**: X2 is the P12 add/delete-shadow case it is missing.

---

## 6. Deferred list

| Item | Waits on |
|---|---|
| Answer's performance budgets (512 MiB, 50 ms, budget table) and Stage F beyond 15.c | Performance after publish (owner, 2026-10-27); baselines dc-hsa5.13, .17, dc-10d9 exist |
| Reader-cache epoch per partition (R10) | 15.c measurements showing it matters |
| Modelling Excel's first-calculation restart and calc-chain order (D2, E6, 3.8 learned order) | dc-hsa5.20.10 decision; dc-hsa5.20.3 typed stop reasons |
| Bare-reference dependent rule in the engine | X4 results (15.d) |
| Fact invalidation for CELL format, filename, sheet order without a token | 15.a, then X6 results |
| Spill-block event placement | X3 results |
| RTD batch atomicity, equal-payload propagation, lease reuse (E12, 3.10 RTD trace) | dc-snz.23, dc-hul4 |
| Native XLL async stale returns against Excel (E13 native part) | dc-6332.13 step 5 |
| External links, query/table refresh, pivot refresh as commands (E14, 3.10 refresh trace, Stage E refresh) | A first refresh provider route; corpus counts 73 external-link failures today |
| Office.js read/calculate contract (E15 JS part) | Harvest 14 (object model and JS) |
| Five-review owner table (3.1) and save/skin/JS publication routes (R8 transport, R13) | Harvests 16 and 17 |
| `ca` on load and save (persistence of volatility) | dc-kapl.2.1, after 15.a |
