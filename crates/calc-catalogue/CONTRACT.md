# calc-catalogue contract (draft)

Read at `origin/master` = `cb0b76a5` (DnaCalc-win), 2026-11-09. Paths are relative to `crates/calc-catalogue/src/`
unless they start with another directory. `path:line` was read at that commit; "(inferred)" was not.

## 1. Purpose and position
The static declarations of every Excel worksheet function, operator and binding form: identity, spelling, arity,
parameters, result form, argument demand and measured call traits, plus the file-format name tables. Data only:
the kernels stay in calc-func (`ported.rs:1`). Portable (`crates/calc-catalogue/Cargo.toml:9`); its one dependency
is `calc-value` (`Cargo.toml:13`). `tools/xtask/architecture.json` names calc-catalogue in none of its 17 rules;
it is bound only as part of calc-func's closure, which must not contain calc-fml, calc-engine, calc-host or
calc-vba (`architecture.json:18-27`) (inferred). Direct dependents (their `Cargo.toml`): calc-func, calc-fml,
calc-engine, calc-doc, xtask-harness. calc-func attaches one kernel per declaration, re-exports the types and
holds no declaration of its own (`crates/calc-func/src/catalogue.rs:17-20,124`). calc-fml reads demand and traits
through calc-func's call target (`crates/calc-fml/src/eval/compile.rs:1161,1188`; `eval/run.rs:641,1407,1426,1767,
2023-2024,2382-2397`; `eval/mod.rs:153,223,365`). calc-engine reads `is_subtotal` through the plan and the file
prefixes (`crates/calc-engine/src/recalc.rs:3011`, `read.rs:1967`); calc-doc reads the file forms.

## 2. Public API by concept
- **Identity.** `FunctionId(u32)` = family code << 16 | member, never derived from an entry's position
  (`declarations.rs:5-28`). `Notation::{Call, Prefix, Infix, Postfix}` (`:38-47`).
- **The entry.** `WorksheetDeclaration` (`declarations.rs:479-495`): `localized_names`, `context_facilities` (both
  `Option`; `None` = not recorded), `id`, `kind: EntryKind`, `spelling`, `notation`, `availability`, `arity`,
  `groups: Option<ArgumentGroups>`, `parameters`, `result: ResultForm`, `demand`, `traits`; builders `with_result`,
  `with_groups`, `with_traits`, `with_three_d` (`:497-516`). An entry without an ordinary kernel is an
  `unheld::UnheldEntry`: the same fields plus `implementation: Implementation` (`unheld.rs:13-30`; `:84-89`).
- **Parameters.** `Parameter { name, class, entry, legacy }` (`:314-319`; builders `:366-399`).
  `ParameterClass::{Value, ValueOrReference, Reference}` (`:261-268`): `Value` means the caller dereferences before
  the call. `EntryClass::{Any, Reference}` (`:404-409`): what Excel's formula entry accepts in that place.
- **EntryTraits** (`declarations.rs:188-216`; `NONE` `:219`; one `const fn` builder per flag, `:241-256`):
  - `intersect_legacy_result`: intersect the result in a legacy formula outside an array place;
  - `reject_multi_cell_reference`: refuse a multi-cell reference at a scalar parameter;
  - `padding_precedes_call`: a missing lifted coordinate gives its error before the kernel runs;
  - `can_skip_nested_subtotals`, `is_subtotal`: set together by `skip_nested_subtotals()` (`:244`);
  - `lift_dynamic_references`: in a dynamic formula, run once per reference of an array of references;
  - `take_reference_array`: read an array of references whole; `lift_computed_array`: lift a computed array even so;
  - `paired_result: PairedResult::{None, Odd, EvenAfterSelector}` (`:93-98`), result positions whose computed-blank
    origin is kept; `whole_result_policy: WholeResultPolicy::{None, Conditions, Selector}` (`:101`);
  - `zero_array_blanks`, `zero_lifted_blank` (`if_blanks()`), `zero_dynamic_lifted_blank` (`choose_blanks()`);
  - `volatile`; without doc comments: `flush_subnormal_inputs`, `retain_overflow_number`,
    `retain_subnormal_output`, `strict_number_arguments`; `three_d: ThreeD`, read by `at(i) -> ThreeDPlace` (`:105-149`).
- **ArgumentDemand** (`declarations.rs:273-309`): `Eager`, `BranchOnCondition`, `FallbackOnError`; `is_lazy()`,
  `discriminator()` (argument 0 for both lazy kinds), `holds_back(index)`. Lazy, and no others: IF `logical.rs:83`,
  CHOOSE `lookup.rs:149` (`BranchOnCondition`); IFERROR `error_fallback.rs:26`, IFNA `:41` (`FallbackOnError`).
- **LegacyPlace** (`declarations.rs:323-364`), 20 variants: `Default`, `IntersectReference`,
  `IntersectReferenceAlways`, `SingleValue`, `ArrayAlways`, `ArrayAlwaysLiftReferences`, `ArrayAtRoot`,
  `AggregateAtRoot`, `AggregateUnderArray` (these five also `...Repeated`), `AggregateAtRootTwice`, `LiftReferences`,
  `AggregateRootArrayNested` (also `...LiftReferences`), `AggregateIntersectReference`, `AggregateAlwaysIntersectReference`.
- **Lookup.** `declaration(id) -> Option<DeclarationRef>`, a `const fn` (`registry.rs:138-192`); `named(spelling)`,
  invariant spelling, calls only, a linear scan (`:201-203`); `declarations()` (`:195-198`); `DECLARATION_COUNT`
  (`:57`). `DeclarationRef` has an accessor per field, plus `implementation()` and the file forms (`:61-135`).
- **Registering a family.** A family file declares `pub const CODE: u16`, a `FunctionId` constant per member and
  `pub const ENTRIES: &[WorksheetDeclaration]` (`logical.rs:4-12,63`); ported families use `data_family!`
  (`ported.rs:4-38`). The family is listed twice in `registry.rs`: `FAMILIES` (`:7-44`) and the `match` of
  `declaration` (`:148-185`). calc-func attaches kernels by id (`crates/calc-func/src/catalogue/kit.rs:21-44`).
- **File-format tables.** `file_prefix`, `file_form`, `EXCEL_FUNCTION_IDS`, `help_of` (`lib.rs:25-97`).

## 3. Guarantees and invariants
- **Static data.** Every table is a `const`; no function here computes a worksheet value (`registry.rs:7,46-57`).
- **One declaration per function, one kernel per declaration.** calc-func's build fails when its entry count
  differs from `DECLARATION_COUNT`, an attachment does not match `implementation()`, or two entries share an id,
  a family code or a notation and spelling (`crates/calc-func/src/catalogue.rs:168-191,253,285-287`). This crate
  alone checks none of that: `declaration` and `named` return the first match.
- **Shape rules**, also checked in calc-func (`catalogue.rs:295-365`): parameters cover the arity; a reference-only
  entry place is never a `Value` parameter; lazy demand has a lazy kernel, eager demand an eager one (`:355-361`).
- **What a declaration promises about its kernel.** `Value`: the kernel never sees a reference there
  (`declarations.rs:262-263`). Lazy demand: only argument 0 is evaluated before the entry decides (`:276-281`).
- **Not enforced.** No code checks that a kernel uses only what its entry declares: `context_facilities` is `None`
  on every entry and `ContextFacility` has no reader outside this crate (grep). `volatile` is not tied to a
  kernel's use of random or host services. A lazy kernel's choice depending only on the arguments it has seen is
  assumed, not declared (`docs/architecture/review-2026-11/reviews/fable-evaluator-review.md:91-93`).
- **Unmeasured stays unmeasured.** `ThreeDPlace::Unknown`, `ResultForm::Unknown`, `Availability::Unspecified` mean
  "not recorded" (`declarations.rs:69,105,416`); unheld signatures are handbook seeds (`unheld.rs:70-71`).

## 4. What it must never do
- Touch `std::fs`, `std::process`, `std::os` or `std::net`, or stop building for wasm32 (AGENTS.md, Portability).
- Depend on anything but calc-value: no kernel, evaluator, graph, document or host package.
- Hold evaluation logic (kernels, coercion, arithmetic on worksheet values). A per-function behaviour is declared
  here as a fact; calc-fml asks for it and never tests a spelling (`docs/architecture/FUNCTION_CATALOGUE.md:9-13`).
- Declare a function twice, or let another crate keep a copy (AGENTS.md, "Extend, never duplicate").
- State as measured what was not, or regenerate a `*-before.tsv` snapshot to make a test pass (honesty rule 3).

## 5. Excel behaviours it owns as declarations
- **Volatile, unconditionally** (10): CELL `ported.rs:324`, INFO `:325`, NOW `:401`, TODAY `:402`, RAND `:603`,
  RANDBETWEEN `:604`, RANDARRAY `:605`, WEBSERVICE `:635`, OFFSET `offset.rs:37`, INDIRECT `reference_text.rs:24`.
  No declaration cites a measurement; the cited source is Microsoft's "Excel Recalculation" page
  (`crates/calc-fml/src/eval/mod.rs:360-362`). SUMIF is not volatile (`conditional.rs:107`).
- **The lazy four**: IF, CHOOSE, IFERROR, IFNA (section 2); no comment cites a measurement of the laziness itself
  (probe P18). IFS and SWITCH are `Eager`, with `PairedResult::Odd` + `WholeResultPolicy::Conditions` and
  `EvenAfterSelector` + `Selector` (`logical_more.rs:29-30,53-54`).
- **Blank results.** IF: selected array blanks and lifted blanks become 0, cited W111 (`logical.rs:84-85`).
  CHOOSE: a lifted selected blank becomes 0 in a dynamic formula, cited "Dynamic fn2 q193" (`lookup.rs:150-151`).
- **Entry classes, groups, result forms**: dc-mx3x, Excel 16.0.20527, 2026-11-07, `Formula2` and `Formula` alike
  (`logical.rs:64-66`, `logical_more.rs:9-13`, `lookup.rs:46-48,153-156`, `offset.rs:13-17`, `conditional.rs:16-22`).
- **Legacy placement rows.** Uses here (by grep): `ArrayAtRoot` 92, `AggregateAtRoot` 79 (+29 repeated),
  `ArrayAlways` 20, `ArrayAtRootRepeated` 6, `ArrayAlwaysLiftReferences` 4 (+1), `AggregateUnderArray` 3 (+3),
  `SingleValue` 2, five others once. Cited: SUM, dc-10y and dc-b1l, Excel 16.0.20527 (`aggregate.rs:20-24`); TYPE,
  dc-o9r (`information.rs:24`); the 2026-10-23 table and dc-m3u rows (`crates/calc-fml/src/eval/legacy.rs:1-45`).
- **Reference arrays.** `lift_dynamic_references`: INDEX `lookup.rs:61`, ROW `:162`, AREAS `:186`, COUNTIF
  `conditional.rs:104`, SUMIF `:107`, SUMIFS `:109`, AVERAGEIF `:110`, N `information.rs:79`, CELL `ported.rs:324`,
  SUBTOTAL `:526`, T `:591`. `take_reference_array`: SUM `aggregate.rs:77`, COUNT `:81`, COUNTA `:82`, AVERAGE
  `:99`, MAX `:102`, ROWS `lookup.rs:174`, ISREF `information.rs:76`. `lift_computed_array`: COUNTBLANK, cited
  W111 and w089 (`aggregate.rs:96-97`). `intersect_legacy_result`: ROW, COLUMN (`lookup.rs:162,168`), FILTERXML
  (`ported.rs:555`). `skip_nested_subtotals`: SUBTOTAL, AGGREGATE (`ported.rs:526-527`).
- **Padding and scalar refusals.** `padding_precedes_call`: ROUNDUP, ROUNDDOWN, cited W111 (`rounding.rs:63-65`).
  `reject_multi_cell_reference` with `strict_number_arguments`: MROUND, QUOTIENT (`rounding.rs:66,70`).
- **Subnormal and overflow** (`rounding.rs:61-69`, no measurement cited there): `flush_subnormal_inputs` TRUNC,
  ROUND, ROUNDUP, ROUNDDOWN, MOD; `retain_overflow_number` TRUNC, ROUND, ROUNDDOWN; `retain_subnormal_output`
  TRUNC, ROUNDUP, ROUNDDOWN. **3-D references**: dc-10y, Excel 16.0.20430, 2026-10-25 (`declarations.rs:107-110`).

## 6. How it is tested
- Here, native and wasm: `old_declarations_match` (`lib.rs:626`; prefix snapshots of 515 and 517 rows), a lookup
  test (`registry.rs:209`), a help snapshot (`help.rs:5010`), a file-call test (`file_call.rs:82`),
  `admission_export_is_current` (`crates/calc-catalogue/tests/admission_export.rs:5`).
- In calc-func, against snapshots in `crates/calc-catalogue/tests/fixtures/`: `catalogue-before-ported.tsv` (548
  rows, every entry at 9edc9030; `crates/calc-func/src/catalogue.rs:747-757`); `ported-parameters-before.tsv`
  (548 rows: class, entry class and legacy place per position; `:801-813`); unique ids and field equality with
  the attached entries (`:762-781`); `offset-before.tsv`, `unheld-before.tsv` (`catalogue/offset.rs:387`,
  `catalogue/unheld.rs:83`). Demand: `crates/calc-func/tests/acceptance.rs:1110-1122,1299-1309,15789-15792`.
- Against Excel: the catalogue oracle, `crates/calc-func/tests/fixtures/catalogue/` (`oracle.xlsx`, 24
  `sheet-*.txt` and 6 `prefix-*.txt` sections), captured per sheet; whole route `cargo xtask corpus --oracle`.
  Volatility: one engine assertion (`crates/calc-engine/tests/acceptance.rs:3671-3672`), no per-entry test.

## 7. Open items
- P17 (volatility matrix: each CELL/INFO argument, matched and mismatched SUMIF ranges) and P18 (branch evaluation
  and error order): probes not yet run, no bead ids (`docs/architecture/EXCEL_CALC_PROBES.md:301-310`).
- REWORK_PLAN R3 (a named `Volatility` policy replacing `volatile: bool`) and R5 (compiled `CallPolicy`, typed
  demand) (`docs/architecture/review-2026-11/REWORK_PLAN.md:38-52`); review R10, R12 (`:281-294`). No bead ids.
- Owners named in code: dc-fetch-f0tp (services, PY) and dc-nk93 (HAS, HASANY, HASALL, FLATTEN)
  (`declarations.rs:60-61`); dc-gh22 (CALL, REGISTER.ID; `unheld.rs:245,253`); dc-snz.23.4 (RTD; `unheld.rs:214`).
- Localised names: no table yet (`docs/architecture/FUNCTION_CATALOGUE.md:189-194`); `context_facilities` unset.

## 8. Findings for the tracker
1. `crates/calc-fml/src/eval/mod.rs:18-20` lists the lazy entries without CHOOSE (`lookup.rs:149`).
2. `crates/calc-fml/src/eval/mod.rs:360-362` lists nine volatile functions; WEBSERVICE is a tenth (`ported.rs:635`).
3. CELL and INFO are volatile for every argument (`ported.rs:324-325`) and SUMIF never (`conditional.rs:107`);
   Microsoft's page makes all three argument-dependent (review `:206-208`). Unmeasured until P17.
4. Candidate (a) does not hold: `crates/calc-func/src/catalogue/reference_text.rs:9-14` now says INDIRECT is
   declared volatile; the review's section 1.10 and D1 (`:200-202,320-322`) describe the earlier comment.
5. The review counts 21 `LegacyPlace` variants (`:103,283`); the enum has 20 (`declarations.rs:323-364`).
6. On no parameter: `IntersectReference`, `IntersectReferenceAlways`, `ArrayAlwaysRepeated`, `AggregateRootArrayNested`
   (`declarations.rs:327,329,335,355`); still matched in `crates/calc-fml/src/eval/legacy.rs:94-95,155-156`.
7. `docs/architecture/FUNCTION_CATALOGUE.md:9-11` puts each declaration in calc-func's family file, and
   `crates/calc-func/src/catalogue.rs:4-5` says unmoved families keep theirs; all are in calc-catalogue (`:178`).
8. `tools/xtask/architecture.json` has no rule for calc-catalogue: a new dependency there fails no layering check.

Verified: every `path:line` above, read at `cb0b76a5` with `git show` / `git grep`; trait and legacy-place lists by grep.
Inferred: the layering bound in section 1; that no entry sets `context_facilities` (grep for `Some`, not a build).
Not read: `help.rs`, `file_function_ids.rs`, `file_form_table.rs`, `file_call.rs`, most `ported.rs` rows, the sheets.
