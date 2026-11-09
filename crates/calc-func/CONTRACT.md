# calc-func contract (draft)

Read at `origin/master` = `cb0b76a5` (DnaCalc-win), 2026-11-09. Paths are relative to `crates/calc-func/src/`
unless they start with another directory; `xn/` is `ported/excel_numeric/`, `cat/` is `catalogue/`, `T/` is
`crates/calc-func/tests/`. `path:line` was read at that commit; "(inferred)" was not.

## 1. Purpose and position
The worksheet function catalogue and its kernels (`lib.rs:1-14`): 36 family fragments (`catalogue.rs:67-104`),
117 files of ported numeric kernels under `ported/`, coercion, exact decimals, number text, formats, locale. It
holds no formula evaluator, no reference grammar and no workbook state (`call.rs:31-32,73-76,247-250`).
Portable (`crates/calc-func/Cargo.toml:10`). Depends on `calc-catalogue`, `calc-value` (re-exported as `value`),
`libm`, `sxd-document`, `sxd-xpath` (`Cargo.toml:18-24`). `tools/xtask/architecture.json:17-27`: its closure must
not contain `calc-fml`, `calc-engine`, `calc-host` or `calc-vba`; the file also forbids calc-func to four others.

## 2. Public API by concept
**Catalogue.** `catalogue()` (`catalogue.rs:120`) is built by the compiler into an ID table and a surface table;
a duplicate, a broken declaration rule or a declaration without a kernel stops the build (`:166-191,241-292`).
`resolve(FunctionId)`, `resolve_surface`, `resolve_addin`, `entries()`; `targets()` and `surfaces()` list held
kernels only (`:430-524`). `FunctionCallTarget` (`:530-710`) gives the metadata, `signature_help`,
`oracle_formula`, `check_entry` (`cat/entry.rs:150`). `Kernel::{Eager, Resumable, Lazy, Binding, NotHeld, Deferred}`.

**The call interface** (`call.rs`).
- `invoke(target, &mut dyn PreparedArguments, &mut EvalServices, &mut FunctionCallScratch) ->
  Result<CalcValue, FunctionFault>` (`:521`): binding, not-held and deferred entries are refused, then the arity
  is checked, then arguments are asked for (`:541-554`). `PreparedArguments::{count, argument}` (`:22-29`).
- Admission (`:656-675`): a `Value` parameter refuses a reference, a `Reference` parameter anything else;
  `#REF!` given to a reference-only parameter is the result (`:641-649`). `FunctionFault` (`:417-451`): `NotHeld`,
  `Deferred`, `Binding`, `ContinuationTarget`, `ArityMismatch`, `Reference{Argument,Required,Unreadable}`,
  `NotImplemented`, `ExactUnavailable`, `MissingReturned`.
- `EvalServices` is here, not in `kernel.rs` (`:281-383`): `new(numeric)` or `with_cells`, then `with_random`,
  `with_reference_text`, `with_environment`, `with_callables`, `with_host`. Caller-owned traits: `CellReader`
  (`:33`), `RandomSource` (`:64`), `ReferenceText` (`:77`), `HostFacts` (`:102`), `CallableInvoker` (`:251`).

**Kernel side** (`kernel.rs`, crate-private). `Exit::{Error(code), Fault(fault)}`, `Step<T>` (`:16-24`);
`Site::finish` makes an `Exit::Error` the call's value and an `Exit::Fault` its fault (`:54-60`). `Site` has
`not_modelled` (`:64`), `number_argument` (`:91`), the sites `binary64`, `add`, `subtract`, `multiply`, `nearest`,
`divide` (`:165-217`) and `number(x)` (`:222`). `NumericMode::Exact` refuses every site (`:154-161`).

**Numeric substrate** (`xn/mod.rs`, crate-private).
- x87 at precision 64, then a binary64 store: `excel_x87_add/sub/mul/div/recip` (`:492-581`). Chains are
  written once over `Fpu` (`:106-123`): `Hard` runs x87 instructions on x86_64, `Soft` is `soft87`; `Native` is
  `Hard` on x86_64 unless feature `soft-x87` is set (`:150-165`; `Cargo.toml:13-15`).
- Leaves: `excel_exp`, `excel_log`, `excel_log10` (`:81-85`), `excel_sin/cos/tan` (`:452-481`), `excel_atan`
  (`:584`); the Windows runtime's functions as `crt_pow`, `crt_exp`, ... (`:87-95`). POWER: `power_kernel`
  (`ported/power_fn.rs:75`) over `excel_pow_positive`/`excel_pow_chain` (`:418-444`).
- Decimals (`decimal.rs`): `ExactDecimal::read` (`:315`), `fifteen` (`:350`), `fifteen_x87` (`:452`), `round_at`
  (`:577`); text to number: `Decimal::nearest` (`coerce.rs:79`). Public: `read_number_text`, `text_order`.

**From entry to kernel** (`cat/kit.rs`). `attach_family!` and `ported_shared_family!` attach a kernel to each
calc-catalogue declaration by stable ID (`:21-115`); a generated kernel refuses an array in a value parameter and
passes `Site::binary64` first (`:102-104`). Readers and publishers: `num`, `opt`, `text`, `grid`, `out` (`:118-433`).

**Resumable kernels and LAMBDA callbacks.** `invoke_slice(.., elements) -> Result<Option<CalcValue>, _>`
(`call.rs:576`) admits the arguments once, keeps a `KernelTask` in the scratch and advances it; `Ok(None)` is
"not finished". Resumable: SEQUENCE (`cat/arrays.rs:632`), MAP, REDUCE, SCAN, BYROW, BYCOL, MAKEARRAY
(`cat/lambda_helpers.rs:456-467`), GROUPBY, PIVOTBY (`cat/grouping.rs:910-911`). A helper calls its LAMBDA by
`CallableInvoker::call_slice`: `Ok(None)` keeps the item, `CallAbandoned` ends in a fault (`call.rs:257-276`).

**User functions** (`udf.rs`). `UserFunction` (`:194-247`): `call`, `call_with_context`, `begin_call`, and
declarations such as `max_parallel_calls`, `cached_reads`. `UserFunctionInvocation::resume` (`:151-157`).
`UserFunctionContext` (`:259-373`): `values_of`, `worksheet_function`, `set_volatile`, `refuse_by`, RTD reads.

## 3. Guarantees and invariants
- **A kernel reaches the world only through its services**: cells, random numbers, reference text, host facts
  and LAMBDA calls. An absent service is a refusal, never a default (`call.rs:31-32,292-293,319-320,335-336`).
- **Two ways to stop.** A worksheet error is a computed result; a fault is not a value, and the caller reports
  the call as not calculated (`kernel.rs:1-3`; `call.rs:414-415`). The catalogue is immutable (`catalogue.rs:11-12`).
- **What a call returns.** Any `CalcValue` except Missing (`call.rs:677-683`); a published number is finite and
  never -0 (`kernel.rs:219-231`). A lazy entry never evaluates an argument its kernel does not select (`call.rs:5-9`).
- **A kept continuation belongs to one entry.** Another entry's call gives `ContinuationTarget` and keeps the
  task (`call.rs:527-532`). A helper never enters a completed LAMBDA call again (`cat/lambda_helpers.rs:303-304`).
- **Subnormals and overflow.** A non-finite result is `#NUM!`. A subnormal result or operand is refused
  (`kernel.rs:219-231`; `coerce.rs:166-173`) unless the kernel holds its own measured rule
  (`cat/kit.rs:165-176,242-252`). An exact decimal that would round to zero is refused (`kernel.rs:192-206`).
- **Bit-exactness and its limits.** The x87 entry points are bit-exact to 64-bit Excel on x86_64
  (`xn/mod.rs:4-7`). The software x87 can differ by one unit of the 64-bit significand in six transcendental
  instructions (`xn/mod.rs:9-15`): 3 LN results in 200,000 (`:917-921`). Four EXP witnesses round the other way
  on both backends (`:922-928`). 15 pinned rows depend on the CPU vendor (`T/f1_support/x87_cpu.rs:1-5,27-29`).
- **Refusals.** `NotHeld` and `Deferred` name an owner; `NotImplemented` only entry and argument (`call.rs:419-444`).

## 4. What it must never do
- Touch `std::fs`, `std::process`, `std::os` or `std::net`, or stop building for wasm32 (AGENTS.md:26-29).
- Depend on calc-fml, calc-engine, calc-host or calc-vba; parse reference text; evaluate a formula.
- Add a file without its entry in `migration/licence-ledger.json` (`path`, `class`, `reason`; AGENTS.md:67-69).
- Use a source outside its tier (`docs/plan/RESEARCH_SOURCES.md:11-16`): tier 3 gives facts only, through an
  analyst; tier 4 (Microsoft source and decompiled binaries) is never read. Measuring a DLL's outputs is allowed.
- Hard-code an answer, weaken an assertion, regenerate a capture to force a pass, commit `todo!()`, or return
  a guess where Excel's result is not measured, instead of a counted refusal (AGENTS.md:127-133).
- Call the platform math library or depend on the target's FMA (`xn/mod.rs:37-45`); copy a shared fixture, oracle
  or adapter instead of extending it (AGENTS.md:96-117).

## 5. Excel behaviours it owns
- **Two-step rounding at stores.** `+ - * /` and the functions on these sites (SUM, PRODUCT, AVERAGE, SUMSQ)
  compute at 64-bit precision (control word `0x133F`), then store to binary64: a result within 1/4096 of a unit
  in the last place of a midpoint goes to the even neighbour (`kernel.rs:150-153,169-174`; `xn/mod.rs:6`).
  Measured: dc-6qki, oracle sheet `dc6qki_arith`; W109 for XNPV and NPER (`xn/mod.rs:488-490,503-504`).
- **Typed numbers have 15 digits.** A binary64 whose shortest decimal has at most 15 significant digits is read
  as that decimal by the rounding functions (`decimal.rs:309-314`), which round to 15 digits first: Excel
  16.0.20430, capture `cfunc_m3_2` (`:345-349`). Above 10^27 the scaling matches 879 of 1,082 rows (`:440-451`).
- **Text read as a number.** Plain decimal text, 15 significant digits, at most 255 UTF-16 units, scale -308 to
  308, else `#VALUE!`: capture `cfunc_m1_2`; some shapes carry the rule unmeasured (`coerce.rs:9-13,70-78`).
- **POWER and `^`.** Exponent 0.5 is the hardware square root, 400 of 400 live rows (`xn/mod.rs:419-430`);
  integer exponents use staged products (`ported/power_fn.rs:34-50`); 10^k is double rounded, shown by 1E+126,
  Excel 16.0.20527 (`ported/power_fn.rs:52-65`); otherwise `exp(RN53(RN64(y ln x)))` (`xn/mod.rs:434-444`).
- **Elementary functions.** SIN reduces by the 64-bit ROM pi with `FPREM1`: 1,020 of 1,020 live rows
  (`xn/mod.rs:446-451`). EXP, LN: clean-room x87 sequences; LN 113 of 113, EXP 132 of 136 (`xn/x87_chains.rs:1-20`).
- **Per-function status** is not listed here. Read `crates/calc-func/witness-status.tsv` (written by the witness
  judge), `witness-differences.tsv` beside it, and `docs/generated/{problem-functions,numeric-fingerprints}.md`.
  Counted from the status file, full bank: 295 functions; 230 exact, 21 last-bit, 25 numeric, 7 gross, 12 other.

## 6. How it is tested
- Unit tests sit in the source files: x87 hardware against software (`xn/mod.rs:944-1005`), call boundary
  rules (`call.rs:697-861`). Through `invoke`: `T/{f1_*,f2_*,f3_*}.rs`; `T/acceptance.rs` with `acceptance.json`.
- The one catalogue oracle: `T/fixtures/catalogue/` (`sheet-*.txt`, `prefix-*.txt`, `oracle.xlsx`), written by
  `T/fixtures/make-catalogue-fixtures.ps1`, captured per sheet in `T/captures/catalogue/sections/` (26 files). A
  changed sheet is "awaiting capture" until `cargo xtask capture --pending` runs (AGENTS.md:100-114).
- Pinned witnesses: `T/witnesses.rs:1-7` judges `T/witnesses/*.json` bit for bit; a regression and a gain both fail.
- `cargo xtask witness judge --bank full` judges the evidence banks (`tools/xtask-harness/src/witness.rs:8-31`);
  `cargo xtask corpus --oracle` judges the oracles; their `--profile perf` counts must equal debug (AGENTS.md:148).
  Recorded, not re-run (`docs/generated/problem-functions.md:11-12`): bank 3,887,883/3,910,773, corpus 75,788/77,561.
- Browser rule (AGENTS.md:63-67): a test carries the two `cfg_attr` lines (`call.rs:814-815`) and embeds its
  data; one that cannot keeps `#[test]` and a `/// Native-only: <reason>` line (`xn/mod.rs:952-954`).

## 7. Open items
- dc-zmk: every in-scope function computes as Excel does. dc-j9k: exact bits in distributions, regression and
  financial solvers (`docs/plan/BULK_PORT_PLAN.md:16-20`), and the ETS seasonal fits (`cat/statistics/ets.rs:1-21`).
- dc-j9k.12: PMT family (`xn/mod.rs:859`). dc-96l: licence review of the x87 chains (`xn/mod.rs:8`). dc-b1l.25:
  number text near a 16th-digit tie (`coerce.rs:27-29`). dc-8wp.19: CUBE across tables (`cat/cube_functions.rs:18`).
- dc-snz.23.4: RTD without a registered connector (`crates/calc-catalogue/src/unheld.rs:213-215`). dc-snz.23 and
  dc-hsa5.4: the default refusals of `UserFunctionContext` (`udf.rs:289-310`). dc-nk93: lists in cells;
  dc-fetch-f0tp: external-service and Python entries (`crates/calc-catalogue/src/declarations.rs:57-63`).
- No bead id is named for `NumericMode::Exact` (`call.rs:445-447`) or localised names (FUNCTION_CATALOGUE.md:17).

## 8. Findings for the tracker
1. `call.rs:564`: the `Kernel::Resumable { run, .. }` arm cannot run, because `invoke` returns at `:533-540` for
   every resumable target; the `run` kernels and `Calls::Direct` (`cat/grouping.rs:315`) are dead code.
2. `xn/x87.rs:9,19` name `super::nt_chains`; the module is `x87_chains` (`xn/mod.rs:80`).
3. `xn/mod.rs:38-41` says no `f64::mul_add` anywhere; `xn/crt_pow.rs:11` and `cat/statistics/ets.rs:38` use it.
4. `xn/mod.rs:61-64` says the entry points have no in-crate caller and allows dead code; 64 other files call them.
5. `coerce.rs:27` names dc-6qki as owner of subnormal numbers; it is closed and the refusal remains
   (`kernel.rs:227`); `import.rs:779` (dc-z8c1) and `crates/calc-catalogue/src/unheld.rs:245,253` (dc-gh22) too.
6. FUNCTION_CATALOGUE.md:9-11,27-28 puts each declaration in the calc-func family fragment; `catalogue.rs:4-5`
   and `cat/kit.rs:5-6` put them in calc-catalogue, which AGENTS.md:20 and architecture.json do not list.
7. `lib.rs:3` says formatting reaches the crate through a service contract; it holds `mod format` (`lib.rs:31`).
8. `ported/acoth.rs:7-9` describes "the decimal literal below"; the constant is written with `f64::from_bits`.

Verified: every `path:line` above, read at `cb0b76a5` with `git show`/`git grep`; bead states from the tracker file.
Inferred: that nothing else reaches a resumable entry's `run` kernel (grep of `Resumable` and three kernel names).
Not read: kernel bodies under `ported/` and `catalogue/`, `format/`, `locale.rs`, `cube.rs`, `fetch.rs`, `soft87.rs`.
