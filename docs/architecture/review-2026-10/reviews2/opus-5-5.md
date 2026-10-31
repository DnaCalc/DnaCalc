# Review round 2: Opus 5.5

Code at DnaCalc-review 5a9348b unless marked; OxVba read with `git show` at HEAD 0ad2ad28 (2026-10-31 16:38).

## 1. Must-fix

**M1. Early-bound VBA state is stale (1.7, step 6b).** "Both now pass `member.name`" holds at calc-vba's pin
c7ce1349, not at OxVba HEAD. 0ad2ad28 carries the compiler's IID, slot and wire signature into
`ComHal::invoke_early_bound` on both runtimes (VM3 `lib.rs:4258-4294`, JIT `helpers.rs:4404`), with no Invoke
fallback for dual calls, and `docs/spec/HOSTING_AND_EARLY_BINDING.md` is the new hosting contract. Each call
still builds a Variant `Vec<DynamicCallArg>` plus a `HashMap` (`lib.rs:4264-4287`) and goes through libffi
(`windows_vtable.rs:6-7,495`); the JIT calls a helper; portable objects get a Variant adapter,
`PortableDispatch::invoke_early_bound` (hal `traits.rs:445-460`). Correction: 6(b) = bump the pin, then
generated typed calls into DNA's in-process `#[repr(C)]` objects on every platform (no Variant vector, no
libffi), emitted as calls in the JIT. Agree with the OxVba lane now that `PortableDispatch` is not the portable
early-bound contract. Typed JIT primary entries (OxVba CORE-7; "the JIT still uses Variant-backed execution
frames") must come before 6(c).

**M2. Exact on soft and foreign CPUs (1.8, 1.9, 5.1, decision 5).** Nothing refuses today:
`excel_numeric/mod.rs:9-15` says soft `F2XM1/FYL2X/FPATAN/FSIN/FCOS/FPTAN` "round the exact value once" and can
differ from hardware. The powers and the exp/log chains run on these instructions (`:4-8`), so refusing
without a vendor model makes browser Exact refuse `^`, POWER, EXP, LN, trig and everything built on them. That
is close to Fable(d)'s "browser Exact: no", so the 5.1 row overclaims. An AMD target with "hardware valid only
when the vendor matches" also makes every Intel user refuse `^`, though Excel follows the local x87 (4.4).
Correction: on native x86-64 the target is the running CPU, always valid. A named vendor model applies to Soft
and to published cross-machine claims. State the browser consequence in decision 5.

**M3. Parallelism claim is false (section 3 "Kept", 5.1 row).** "Only staged spills stop parallelism
(`recalc.rs:739-757`)" is wrong. `eligible` also requires no iteration, no cycle, a silent observer, known
readers and `definitions().data_tables().is_empty()` (`:745-757`), so one data table serialises the workbook; a
waiting array serialises the rest of the run (`:710-727`). Restate; 3b's table units must lift the exclusion.

**M4. Block membership is not its extent (1.2).** xlsx `ref` is a bounding area. A member is a cell carrying `si`,
and the loader checks only that it lies inside (`load/sheet.rs:680-682`), so sparse groups load today. Either a
block carries membership (dense, runs or bitmap), or load splits groups into dense rectangles and keeps
`si`/`ref` as a save hint. Step 5's save-back depends on which.

**M5. Name memoisation predicate (1.2).** "A name without relative references is one node" is wrong. `=ROW()`,
`=CELL("address")`, `=INDIRECT("A1")` (the caller's sheet) and `=!A1` all depend on the caller. Memoise only
caller-independent names, judged by `FunctionId` facilities plus reference kinds. The memo is a value **or a
reference**: `=Sheet1!$A:$A` and `=OFFSET(A1,0,0,COUNTA(A:A))` keep the area, or ROWS/INDEX/OFFSET over the
name break and a column-sized name materialises 1M values. Put this in decision 4(a).

**M6. Negative watches by caller sheet (1.2, S3)** miss `Sheet2!Foo` on other sheets (`read.rs:214-217`); key them by `(scope, folded name)`.

**M7. Volatiles under `merge_edit` (1.3).** Draws are keyed on (seed, tick, cell) (`random.rs:4-8`). An edit merged
into an in-flight recalculation without a new tick leaves settled RAND/NOW cells unchanged, unlike Excel.
In Automatic mode `merge_edit` re-dirties the volatile set under a new tick.

**M8. "One Cargo edge" vs "static composition" (decision 1).** The host API has "no runtime", but today the runtime
comes in through calc-vba -> oxvba-embed (`calc-vba/Cargo.toml`). Something must link VM3, the JIT and
Cranelift. Name it: calc-* crates depend only on the host API, and each app binary (`dna-calc-worker`,
`dna-onecalc`) has one composition edge to an OxVba runtime facade. Otherwise the rule is false, or loading is
dynamic, which wasm cannot do.

**M9. "Formatting module vs crate: packaging only" (5.2 H).** This decides the page bundle. dna-formula-ui
formats through `calc_func::locale` (`result.rs`; `apps/dna-calc/src/cells.rs:173-192`). With formatting in
calc-fml, any page that renders a number links calc-fml -> calc-func, every kernel included, so 1.6's "page
stops carrying the engine" drops only calc-engine. Correction: a `calc-format` crate (value + numeric digits)
that calc-fml injects as `NumberText`. calc-func does not depend on it (4.1 holds). The page side then needs
only calc-syntax + calc-function-meta + calc-format.

## 2. Missing (next three months)

- **Width-dependent text**: General's digits and `####` depend on column width and font metrics, but every cell
  renders "as a cell of standard width" today (`cells.rs:53`). The owner is calc-host's resolver, with metrics
  from the platform (OffscreenCanvas in the worker). Add a presentation-only invalidation (resize, zoom, font)
  that re-renders tiles without recalculating.
- **Completion before save and macro reads**: under partial publication, save (`save/sheet.rs:25-31`),
  `Application.Calculate` and a macro's `Range.Value` need demand-all plus drain at one tick. Add this to 1.3.
- **Range arguments**: OxVba's Excel probe shows a referenced cell arrives as a `Range` (VarType 5). Shims need
  allocation-free Range objects (activation arena, no 4,096 cap). A Range kept in module state outlives the
  call (lease), and its reads are dynamic reads.
- **UDFs are effectful by default**: same-value inputs still call them (`VBA_UDFS.md:78-80`); no cutoff over them.
- **`calcvba-host-api` = OxVba's "Required host-neutral interface"** (HOSTING_AND_EARLY_BINDING.md); adopt it.
- **Notebook order over names**: workbook.xml has no name order. Persist the order so it survives an Excel round
  trip (custom XML part), before 4.8's notebook ships.
- **Tables share the names namespace**: tables are looked up by spelling (`reference/bind.rs:46`,
  `read.rs:251`). Cover table names in NameTable's folded index, with TableId/column binding.

## 3. Open disagreements (5.2), final positions

A: chunked blocks with membership (M4), member bitset, affine edges; GPT's identity rule. B: 3a then 3b, with 3b
over a unit trait that block chunks implement (section 5). C: ready counts + height heap in 3b. D: no Excel mode
drops off-interest work. E: `tools/calc-func-lab`. F: `calc-object-model` crate. G: measure once the mirror and
RenderPlan exist; accessibility is a requirement, not a tiebreak. H: crate (M9). I: sweep, plus the Cargo-edge
check on `Cargo.toml` changes. J: generation bump.

## 4. Owner decisions (section 6)

1. Agree, amended (M8): one library edge to the host API plus one composition edge per app binary.
2, 6, 8. Agree.
3. Agree on the shape; disagree with it being fourth on one lane (section 5).
4. (a) Agree with M5 (caller-independent; value or reference); (b) agree, plus a persisted order; (c) agree.
5. Disagree as framed (M2): native target = running CPU; a named model for Soft and published claims; refusal
   rule yes, with the browser cost stated.
7. Agree on scope, not timing: the browser never uses asupersync, so 3a's actor is built on calc-sched anyway.
   Spike it off the critical path (RTD/async UDFs, lab replay).
9. Agree. The owner question is whether DNA's `.tlb` reuses Excel's library GUID, IIDs and DISPIDs, so existing
   "Microsoft Excel 16.0 Object Library" references resolve.
10. Agree, limited to step 0 and the command-model part of step 4.

## 5. Implementation order

- **Step 1 is too big.** In order: 1a `SemanticEnv`/`VolatileServices` (small; the clock fix, `state.rs:782-788`);
  1b calc-function-meta; 1c FEC + caller + forwarders in one change; 1d calc-format; 1e `CallableTable` (with 6c);
  1f presentation resolver.
- **CF overlay in step 1, typed CF/DV in step 5**: CF is parsed from XML at query time
  (`calc-host/src/conditional.rs:712`). Move typed CF/DV into 1f, or the overlay into 5.
- **Decision 3 says "now"; blocks are fourth on one lane.** Split step 5. 5a (calc-syntax, blocks in doc-model,
  group load/save, Rgce to templates) is model/codec work and starts now, alongside 1. 5b (affine edges,
  member dirty sets, result columns) lands before or with 3b.
- **3b before 5 builds the scheduler twice** (ready counts, height heap, unit validation on scalar nodes). Run 5b
  first, or write 3b over a `SchedUnit` (node | chunk).
- **3a and step 4 both restructure the calc-host session**: land the host command enum (the actor's inbox) first.
  **Names (2) need not wait for 3**: after 1c the work is the binder and doc-model; give it a second worker.
- **Step 0 counter-slope checks "when engine or host crates change" run per commit**: move them to sweep unless
  they take seconds (4.6).
- **Step 6**: pin bump and contract adoption first. (b) is partly landed (M1). Run (c) in parallel with (b): the
  UDF hot path (owner's first bullet) depends on (c), not on Windows COM exposure (d).
