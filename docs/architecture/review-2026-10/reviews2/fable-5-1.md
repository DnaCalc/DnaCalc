# Review round 2 by Fable 5.1

Checked SYNTHESIS.md against DnaCalc-review 5a9348b (`git show`), OxVba HEAD 0ad2ad28 (2026-10-31 16:38,
after every round-1 review; the reviews read c7ce1349), the plan and SYMBOLIC_SEARCH.md at dbcc791. Nothing
built or run. Rulings 4.7 and 4.8 taken as given.

## 1. Must-fix

1. **1.7 and 6(b) are stale and cite OxVba without a commit.** At 0ad2ad28 VM3's `ComCallEarly` on a COM or
   portable object calls `host.com().invoke_early_bound(object, &descriptor, args)` (`oxvba-vm3/src/lib.rs:
   2186-2192, 4258-4305`); the JIT passes an `early_member` pointer into `project_member_get_slot` and the
   helper takes the same HAL entry (`oxvba-jit/src/helpers.rs:4397-4403`); the Windows bridge executes
   "compiler-owned metadata directly. No live type-info lookup, GetIDsOfNames" and dual members go through
   vtables (`oxvba-com/src/windows_bridge.rs:89-93`); the portable trait refuses a name-based retry
   (`platform/portable.rs:44-53`). Only a VBA class instance is still dispatched by `member_name`
   (`lib.rs:2186-2187`). Correction: "name-keyed in both runtimes" becomes "descriptor-keyed, but every call
   still boxes its arguments as `Vec<DynamicCallArg>` Variants at the HAL"; 6(b)'s remaining work is the
   typed-slot HAL entry and the JIT's direct vtable call, not the lowering. Pin the OxVba commit in SYNTHESIS.
2. **Section 3 "calc-doc: drop calc-fml edge" fails with calc-syntax as defined.** The crate map gives
   calc-syntax "lexer, tokens, templates, A1/R1C1 rendering, reference transforms". calc-doc needs the
   parser: `save/file_form.rs:40` imports `parse_formula`, `SyntaxNode`, `SyntaxKind` and walks the tree
   (`:433, :694`) to decide dynamic-array storage; `edit.rs:160` parses for diagnostics. Correction:
   calc-syntax owns the whole of `calc-fml/src/syntax/` (lexer, parser, tree), or calc-doc keeps the edge.
3. **1.2 "a name without relative references is one node with a memoised result" is wrong for most names.**
   Most names evaluate to references (`Sheet1!$A$1:$A$100`, `$A:$A`); a memoised value breaks `INDEX(Name,5)`
   as a reference, `ROW(Name)`, `Name 3:3`, `@Name`, COUNTBLANK, SUBTOTAL exclusions, CELL("address",Name),
   and memoising `$A:$A` copies a million cells on every change. Correction: a name node publishes a
   *reference-or-value* plus its binding version; a reader of a reference-valued name takes edges to the
   referenced cells (through the name's version), and only value-valued names (constants, computed arrays,
   LAMBDAs) memoise. Decision 4(a) should be worded that way.
4. **1.2 "covered cells hold no formula entry" assumes every cell of `extent` is a member.** In xlsx the
   `ref` is a bounding range and membership is each cell's own `si`; the loader already works per cell
   (`load/sheet.rs:619-704` records members individually and only checks they lie inside `ref`), and in
   Excel overwriting one member leaves the group's `ref` and takes that cell out. Correction: `FormulaBlock`
   carries a membership mask (or is split into full rectangles at load), the save path writes `si` only for
   members, and decision 8 gains a one-file measurement: does Excel write non-member cells inside `ref`.
5. **1.8/1.9 "Hardware valid only when the running CPU vendor matches the target" contradicts 4.4.** The
   measured fact is that Excel follows the CPU's x87; the same program on the same hardware is therefore
   Excel-on-this-CPU, on Intel as on AMD. Refusing hardware x87 on Intel in favour of `Soft(AMD)` would give
   an Intel user bits their own Excel does not. And `Soft(VendorModel)` does not exist yet: soft87 rounds
   transcendentals once where microcode rounds wider, so it is not AMD-exact either
   (`excel_numeric/mod.rs:9-15`). Correction: natively, hardware x87 is always the Exact path and the CPU
   vendor is provenance on every result and witness; witnesses compare only on a matching vendor; the browser
   refuses unmodelled transcendentals until a vendor model is identified. Decision 5 restated below.

## 2. Missing

- **The CalcVBA lane is building what the architecture removes.** 0ad2ad28 adds 186 lines to
  `oxvba-embed/src/spreadsheet.rs`, the surface 1.7 moves to `calc-object-model`. Nothing in section 4 stops
  that growth before 6(a) lands; make "no new member in `spreadsheet.rs`" the lane's rule today, and cut
  `calcvba-host-api` with a typed-slot call entry from the start, or 6(c)'s shims re-box through
  `DynamicCallArg`.
- **Block detection at load for files without shared groups.** Other writers (and Excel after a paste of
  values-plus-formulas) store a 1M-row fill as 1M `normal` formulas. S2 coalesces equal templates only after
  edits; the loader needs the same coalescing, or the 1M-node case survives for every non-Excel file.
- **`Definitions` is cloned whole on any format, pane, rule or iteration edit** (`root.rs:657-683`), not only
  on a name edit; S3 fixes names. The struct needs per-table `Arc`s in 3a, where the O(workbook) list is.
- **Demand at open.** Loaded cells are `FileCached`; Automatic mode in Excel recalculates volatiles at open
  and everything under `fullCalcOnLoad`. The contract needs the open-time demand set stated, or the first
  visible tiles flicker from file values to results under visible-first publication.
- **Loader fragility a block model inherits**: a `t="shared"` dependent that carries text is read as a
  second master (`load/sheet.rs:626-637`); third-party files do this. One-line fix, but it belongs in 5a.

## 3. Open disagreements (section 5.2)

- A. Block storage with member bitset, affine edges, 64-row chunks; GPT's member-identity rule. Agree.
- B. 3a (actor, budget, whole publication) then 3b on scalar nodes; blocks' engine half after. Agree.
- C. Ready counts and a height heap in 3b. I withdraw "measure first": the counters cost nothing extra once
  units exist, and the first-visible-cell latency is the product claim.
- D. No dropping of off-interest work in any Excel mode; interest orders, demand completes. Agree.
- E. `tools/calc-func-lab`; SYMBOLIC_SEARCH.md:78 "within xtask" read as the repository. Agree.
- F. `calc-object-model` crate, two backends. Agree (moved in round 1).
- G. Undecidable until RenderPlan and the tile mirror exist; measure then. Agree; the accessibility overlay
  is a hard requirement on Canvas2D, not a trade.
- H. Formatting as a calc-fml module. Agree.
- I. Layering in sweep; Cargo-edge check only on commits touching a `Cargo.toml`. Agree.
- J. Reload by generation bump with leases; no in-place swap. Agree.

## 4. Owner decisions (section 6)

1. One Cargo edge calc-vba -> `calcvba-host-api`. Agree; the API must carry typed-slot entries (2 above).
2. Retire atomic generation (plan 2.3 at `:258-264`) for validated units, 3a then 3b. Agree.
3. Blocks now. Agree, but ask for the model-and-codec half first (5a below); the engine half waits for 3b.
4. Names. Agree to (b) and (c); (a) only as reference-or-value nodes (must-fix 3).
5. Exact. Disagree with the shape: Exact natively is hardware x87 on any x86_64 with the vendor recorded;
   published claims and witnesses are per vendor (AMD is the measured one); the browser refuses unmodelled
   transcendentals and counts them as fails. Same refusal rule, no vendor-match restriction on hardware.
6. `tools/calc-func-lab` on calc-numeric and calc-verdict; xtask free of product crates. Agree.
7. asupersync scope. Agree.
8. Three measurements. Agree; add the fourth: non-member cells inside a shared `ref` (must-fix 4).
9. COM introspection of a running Excel as black-box observation. Agree.
10. Pause UI feature slices. Agree; extend the pause to `oxvba-embed/src/spreadsheet.rs` members.

## 5. Implementation order (section 4)

- **Step 1 is five changes sold as one.** "In one change" is right for the FEC itself (forwarders, the
  `call.rs:385-398` fallback, caller from the instance, the data-table decorator). `SemanticEnv` vs
  `VolatileServices`, `CallableTable` slots, calc-function-meta, and formatting-out plus the presentation
  resolver each ship alone and have their own shipping claim. Split 1 into 1a-1e in that order.
- **Step 5 is too big and half of it is in the wrong lane.** 5a: calc-syntax (with the parser), templates
  and blocks in the model, shared groups loaded as blocks and saved back, xlsb Rgce to templates, with the
  engine still expanding members at `begin_recalc` through the `InputView`. This touches no engine
  scheduling and can start after step 0 in a codec lane, which is where decision 3's urgency lies. 5b: affine
  edges, member bitsets, result columns, straddle splitting, after 3b. Style runs and typed CF/DV do not
  belong to blocks: typed CF/DV lands with the presentation resolver (1e, S12), style runs with 3a's
  O(workbook) removals.
- **Step 2 splits the same way**: `NameTable`, `NameId` binding and the `Definitions` split before 3; name
  result nodes and negative watches with 3b, since they are publication units.
- **Step 6(b)** starts from 0ad2ad28 (must-fix 1): the lowering exists; the work is the typed HAL entry and
  the JIT vtable call. 6(a)'s exit condition is `spreadsheet.rs` frozen and `calcvba-host-api` published.
- **3a's list** gains `root.rs:657-683` (whole-`Definitions` clone) and the open-time demand rule.
