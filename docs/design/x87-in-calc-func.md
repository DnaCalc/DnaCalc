# x87 in CalcFunc

The default x86-64 build runs x87 wherever a kernel reaches the `Native` backend. A wasm32
build, an ARM build, and an x86-64 build with `soft-x87` run the same operation sequences
through `Soft`. They preserve the extended intermediates and the locations of binary64
stores. For arithmetic steps this can reproduce double rounding. For a transcendental
instruction the software and hardware answers can differ, even after a binary64 store.
The selection is made at compile time in
`crates/calc-func/src/ported/excel_numeric/mod.rs` (`Fpu`, `Hard`, `Soft`, `Native`).

## Worksheet paths

The table lists where catalogue kernels reach these helpers. A listed family can have
input-dependent paths through more than one column, as well as early publication or domain
checks. Legacy aliases that dispatch to the same kernel inherit that kernel's paths.
Paths in this table are relative to `crates/calc-func/src/`.

| Worksheet functions or entry paths | x87 backend sites | Portable and binary64 sites | Source |
|---|---|---|---|
| EXP, LN, LOG10, LOG, POWER and the power operator | EXP uses F2XM1 and FSCALE. LN and LOG10 use FYL2X with different multipliers. LOG uses LN and a staged division. Nonintegral POWER reaches the LN/product/EXP chain and negative powers can use a staged reciprocal. Its integer lane multiplies with x87 staging. | Domain/zero/identity exits and the positive half-power's binary64 square root. The operator has its own small-exponent paths. | `ported/{exp_fn,ln_fn,log10_fn,log_fn,power_fn}.rs`, `catalogue/arithmetic.rs`, `ported/excel_numeric/{mod,x87_chains}.rs` |
| SIN, COS, TAN, COT, CSC, SEC | FPREM1 reduction with the x87 π constant. SIN uses FSIN. COS uses FCOS or a reconstruction involving FPTAN and FSQRT. TAN uses FPTAN, with a reciprocal in extended precision in odd quadrants. COT, CSC and SEC stage reciprocals. | COS publishes 1 for small arguments before reduction. | `ported/{sin,cos,tan,cot,csc,sec}.rs`, `ported/excel_numeric/mod.rs` |
| ATAN, ATAN2, ACOT, ASIN, ACOS, ACOSH, ASINH, ATANH, ACOTH, FISHER, FISHERINV | Circular inverses use FPATAN directly or through the reduced-angle route. ASIN stages the square root and ACOS subtracts ASIN from π/2. Hyperbolic inverses and FISHER can reach FYL2X. FISHERINV reaches the EXP chain. | Axis, small-magnitude, special-case and publication paths include ordinary binary64 operations. ACOSH uses a binary64 square root before LN. | `ported/{atan,atan2,acot,asin,acos,acosh,asinh,atanh,acoth,fisher_fn,fisherinv_fn}.rs` |
| SINH, COSH, TANH, CSCH, SECH, COTH | SINH uses staged subtraction of two x87 EXPs in its wide lane and an EXP/LN-based `excel_expm1_internal` pair in its central lane. COSH uses staged addition of EXPs. TANH composes SINH and its own denominator, using staged arithmetic. The reciprocal functions reuse these kernels. | The small-argument correction can return the input directly. The remaining ordinary operations include the subtraction and division in the central SINH graph. | `ported/{sinh,cosh,tanh,csch,sech,coth}.rs`, `ported/excel_numeric/mod.rs` |
| IMARGUMENT, IMSQRT, IMSIN, IMCOS, IMTAN, IMCOT, IMSEC, IMCSC, IMSINH, IMCOSH, IMSECH, IMCSCH, IMEXP, IMLN, IMLOG10, IMLOG2, IMPOWER | Argument, square-root and circular/hyperbolic graphs reuse real x87 kernels. Several other cases depend on the axis: numeric IMEXP in the measured range, real-axis IMLOG10 and IMLOG2, and on-axis IMPOWER have x87 sites. | IMLN uses the CRT log and angle throughout. The CRT exp/log/trig/pow helpers serve other complex branches. For example IMPOWER has a CRT angle step on both routes and uses CRT pow and trig off-axis. Component arithmetic and text publication have their own rounding points. | `catalogue/engineering.rs`, `ported/complex_family.rs` |
| BESSELJ, BESSELY, BESSELI, BESSELK | The order/range branches in the Bessel kernels can reach staged recurrences and real trig, log and exp primitives. | Range-specific polynomial, rational and recurrence evaluation also uses binary64. | `ported/{bessel,bessel_rational}.rs` |
| NORM and LOGNORM distribution/inverse families, PHI, ERF/ERFC, CONFIDENCE variants | Staged polynomial arithmetic, x87 EXP tails, LN and staged roots for inverse tails, depending on the entry and argument. | Ordinary binary64 steps and CRT helper calls also occur in these implementations. | `ported/{normal_dist_common,normal_log_family,phi_fn,special_dist_family,confidence_test_family}.rs` |
| GAMMA, GAMMALN, GAMMALN.PRECISE, BETA/GAMMA/CHISQ/F/T distribution and inverse families | GAMMA/GAMMALN reach extended polynomial/seed stages on selected ranges. Distribution families reach combinations of x87 exp/log/pow, staged arithmetic and square roots, and extended beta/gamma series and continued fractions. Test statistics can reach these through the tail kernels. | Binary64 and CRT exp/log/pow are present in individual branches. The large-shape beta correction also has Arm-based log1p sites. | `ported/{beta_gamma_stats_family,chi_f_t_family,special_dist_family,special_math_common}.rs`, `ported/excel_numeric/{gammaln,x87_chains}.rs`, `catalogue/distributions.rs` |
| BINOM, NEGBINOM, POISSON, HYPGEOM, EXPON and WEIBULL distribution families | Binomial and Poisson mass paths include extended deviance/Stirling and x87 exp/log, with compensated exponent graphs on selected paths. Hypergeometric mass includes staged products/divisions. EXPON and WEIBULL reach the x87 exponential family. | A binomial cumulative shortcut combines Arm-based `excel_log1p` with x87-based `excel_expm1_internal`. | `ported/{discrete_dist,special_dist_family}.rs` |
| PV, FV, PMT, NPER, RATE, NPV, IPMT, PPMT, CUMIPMT, CUMPRINC, ISPMT, IRR, XNPV, XIRR, MIRR, PDURATION, RRI, NOMINAL, EFFECT | Time-value and rate kernels use staged products, differences, reciprocals and sums. Various branches also reach x87 power, log, or exp, including PMT's staged expm1 correction. | CRT power and portable exp/expm1/log1p serve selected paths, including period/principal calculations. These functions can mix backends within a single result. | `ported/{financial_time_value_family,cashflow_rate_family,cumulative_finance_family}.rs` |
| Bond and depreciation kernels | Selected PRICE, YIELD, DURATION, odd-coupon, accrued-interest, discount/yield and treasury-bill branches reach staged arithmetic, power or roots. SLN, SYD, DB and DDB have staged arithmetic and the declining-balance kernels can reach power. | Date/day-count, coupon schedule, rounding and pricing branches include ordinary binary64 and non-floating work. | `catalogue/{bonds,financial}.rs`, `ported/{bond_core_family,odd_bond_family,discount_bill_yearfrac_family,depreciation_family}.rs` |
| Basic operators, aggregates, criteria, comparisons and rounding | `Site` stages +, −, ×, ÷. Its callers include SUM, PRODUCT, SUMSQ, AVERAGE/A, SUMPRODUCT, conditional sums/means, database sums/means/products, SUBTOTAL branches, PERCENTOF, comparison tolerance, parity and multiple-based rounding. | Count, error, reference, exact-decimal and empty paths can avoid those arithmetic sites. Text-to-number coercion can separately reach the 15-digit x87 scaling graph. | `kernel.rs`, `catalogue/{arithmetic,aggregate,conditional,database,subtotal,group_pivot,comparison,information,rounding}.rs`, `value_text.rs` |
| Other arithmetic and statistical consumers | Examples include SQRT (staged FSQRT), RADIANS, COMBIN, PERMUT, PERMUTATIONA, MULTINOMIAL, CONVERT, MINVERSE, variance and paired/moment statistics, regression and ETS, and GEOMEAN's root. | Ordinary binary64, exact-integer or decimal work remains in adjacent cases. The staged root differs from a binary64 square root at some midpoints. | `catalogue/{math,statistics,statistics/ets}.rs`, `ported/{sqrt_fn,radians,combin,permut_fn,permutationa_fn,multinomial,misc_conversion_family,matrix_family,variance_common,paired_stats_common,moment_stats_family,regression_forecast_family}.rs` |

The rest of the catalogue uses its own binary64, exact arithmetic, text, reference or host
operations when no path above is reached. The operation classification is separate from the
argument reader: a numeric text argument can enter `value_text.rs` through `Site::read_text`
even for a worksheet kernel whose arithmetic never reaches x87. See the dispatch fragments
in `crates/calc-func/src/catalogue.rs` and the individual kernels under
`crates/calc-func/src/catalogue/`.

Three kinds of software numeric work matter here:

1. **Software x87.** `soft87.rs` models the extended format, precision and rounding fields,
   loads and stores, ROM constants and FPREM1. It evaluates transcendentals internally at
   higher precision and rounds once to the instruction's precision field. For example,
   `RN53(RN64(a × b))` still has two rounding points on wasm. Hardware transcendental
   microcode can deliver a different extended significand, and a difference at a binary64
   midpoint can reach the cell. Source: `crates/calc-func/src/ported/excel_numeric/soft87.rs`
   and `x87.rs` beside it.
2. **Portable infill.** `exp_portable` and `log_portable` use the Arm optimized-routines
   tables in Rust without FMA. `excel_expm1` and `excel_log1p` build on that substrate.
   These identifiers differ from `excel_expm1_internal` and `excel_expm1_spilled`, which
   compose the x87 exp/log graph. The current direct infill callers are in
   `crates/calc-func/src/ported/{financial_time_value_family,discrete_dist,special_math_common}.rs`.
3. **CRT and binary64.** `crt_libm.rs` and `crt_pow.rs` implement separate portable runtime
   graphs. An ordinary binary64 operation or `sqrt` likewise does not select the x87 backend
   (for example binary64 `sqrt` in POWER's half-power path). Native x86-64 can use SSE2 at such
   sites, and wasm uses its binary64 arithmetic. Worksheet SQRT calls `excel_sqrt_staged`.
   Source: `crates/calc-func/src/ported/excel_numeric/` and `ported/sqrt_fn.rs`.

## What the witnesses establish

Selected **full** bank rows, expressed as matched/rows, from
`crates/calc-func/witness-status.tsv`. The status file retains the measurement revision
and Excel builds per row. A family total combines argument paths and cannot identify which
instruction caused a miss.

| Path illustrated | Recorded matches |
|---|---|
| Native x87 chains | EXP 2,456/2,456, LN 1,685/1,685, LOG10 2,823/2,823, POWER 62,635/62,635, SIN 2,129/2,129, COS 3,684/3,684, TAN 2,114/2,114 |
| Hyperbolic compositions | COSH 33,550/33,550, SINH 33,547/33,550, TANH 39,025/39,028. The six misses are last-bit cases. |
| Mixed distribution graphs | BETA.DIST 1,931/2,822, GAMMA.DIST 77,946/77,975, CHISQ.DIST 4,230/4,276, F.DIST 3,263/3,314, T.DIST 4,239/4,281 |
| Real-axis complex samples | IMTAN and IMCOT each 1,209/1,209. Off-axis evidence is recorded separately in `docs/research/imtan-imcot-off-axis-2026-11-16.md`. |

A native Linux x86-64 run of `software_chains_against_the_hardware`
(`SOFT87_N=100000`, `crates/calc-func/src/ported/excel_numeric/mod.rs`) found one
different published bit pattern each for ATAN and SIN, and zero in the sampled
reduced ATAN, COS, TAN and staged SQRT paths. The arithmetic test compared five
operations over 400,000 operand pairs with zero hardware/software disagreements.
These are backend comparisons on one host. The sampled ATAN path is a primitive
chain and differs from the reduced path used by the worksheet ATAN wrapper.

The code and aggregate status cannot assign the mixed-graph misses to the portable
infill. In particular, the premise that SINH and TANH always use it is stale. To identify a
site, compare a frozen witness on both primitive graphs, with an Excel reading whose operands
separate the two results. Keep that evidence by branch and by Excel build.

The Linux vendor fixture pins 13 LN, one EXP and one COS input with different AMD and Intel
hardware results. Excel 20527 on the AMD host agrees with the AMD result on all 15. For
example EXP input bits `c00aff210cf81b26` give AMD `3fa186f1e0bf9266`, Intel
`3fa186f1e0bf9265`, and software `3fa186f1e0bf9266`. The first listed LN example instead
has the software result on the Intel side. These are specific test inputs. The July Intel
18429 and AMD 20131 Excel readings compare different builds, so a same-build Intel/AMD
capture is still valuable (dc-ymjf.7). Sources:
`crates/calc-func/tests/f1_support/x87_cpu.rs` and `docs/plan/WORK_SETS.md` item 4.

### Excel pins for the circular x87 chains

These tests replay exact Excel answers from Windows x64 on an AMD Ryzen 7 4800H
(`AuthenticAMD`). The source applies to each row in the named fixture.

| Test | Excel build | Rows |
|---|---|---|
| `f1_math::dc_j9k_87_cb16_unified_fptan_circular_substrate` | 16.0.20527.20196 | SIN, COS and their real, complex and Bessel callers |
| `f1_engineering::dc_j9k_85_cb15_bessel_high_asymptotic_and_backward_recurrence` | 16.0.20527.20196 | BESSELJ/BESSELY, including high seeds that use the circular chains |
| `trig_matches_live_excel_pinned_witnesses` | 16.0.20131 | W109 SIN/COS/TAN and the reciprocal functions, including COS threshold controls |
| `cot_matches_live_excel_recip_tan_pins` | 16.0.20326 | COT through TAN |
| `csc_matches_live_excel_recip_sin_pins` | 16.0.20326 | CSC through SIN |
| `sec_matches_live_excel_recip_cos_pins` | 16.0.20326 | SEC through COS |
| `t_inv_2t_df1_matches_live_excel_tan_pins` | 16.0.20326, CV2 | T.INV.2T with one degree of freedom through TAN |

Replay these rows on software x87 on any host, and on native x87 when CPUID leaf 0
reports the capture vendor. Vendor matching is the current test rule. Microcode can
also differ between processor models within a vendor. The W109 unit test directly
exercises the software chain in every build. The integration tests run through
`calc_func::invoke` with the backend selected for that build. Run
`cargo test -p calc-func --features soft-x87` to compare all seven fixtures on x86-64
with the software backend. On wasm32 that backend is active by default.

`exp_and_ln_against_live_excel` and `every_pinned_witness_row_calculates_as_pinned`
already use the 15-row vendor fixture where measured hardware results differ.
They retain the AMD Excel pins and the separate measured Intel hardware and
software expectations for those inputs. Thirteen of the known LN rows differ
from AMD Excel on the software backend. The same-build Intel Excel comparisons
and CPU provenance for older captures are tracked in dc-ymjf.7.

## Policy choices

The current `NumericMode` is caller-owned. Its `Fast` path does not select the x87 backend
and can already use native x87. An `Exact(target)` request reaches a typed unavailable
result at the numerical `Site`. The current build feature selects software for the entire
process and is useful for measurement. Sources: `crates/calc-value/src/numeric.rs`,
`crates/calc-func/src/kernel.rs` and `crates/calc-func/src/ported/excel_numeric/mod.rs`.

| Policy | What it gives | Requirement before a parity claim |
|---|---|---|
| Excel on the local CPU | Use native x87 for the measured Excel graphs on x86-64 and keep fixed CRT kernels where the evidence calls for them. A workbook can recalculate to different last bits on another CPU. | Key observations by Excel build/channel, architecture, CPU model and relevant numeric environment. Check vendor-differentiating inputs in Excel on the target, alongside ordinary public-function witnesses. |
| One portable answer | Use `Soft`'s once-rounded extended instructions on all supported targets, with the same complete worksheet graphs and fixed binary64/CRT sites. | Pin the rule and compare full-result bits on native and wasm with the same bank and edge cases. Report the disagreements with each independently captured Excel target. |
| One named CPU's Excel answer everywhere | Reproduce a chosen and versioned target in software, for example the AMD Ryzen 7 4800H environment. | Obtain an instruction model and same-build Excel evidence broad enough to substantiate it. The available rows can test a model. They cannot define its answers on unseen inputs. |

For portability, start evaluation of the once-rounded software policy with the existing
`soft-x87` feature. For CPU parity, retain the local hardware path as an explicitly
identified target while dc-ymjf.7 gathers the Intel and AMD readings. Make any future choice
a setting on the workbook/session or on an explicit request through the existing numeric
context. A host can choose a default and supply its capabilities. Preserve the chosen policy
through nested calls, asynchronous work and recalculation. Apply a changed policy to the
whole calculation, so results and cached dependents have one provenance. If the selected
Excel environment is unavailable, return the typed unavailable result.

Expose the requested policy, the backend and its provenance with calculation results and
witness reports. The handbook can then give matched/total counts for a named Excel target
and for the portable policy separately, alongside the first known differing input and the
observed bits. The choice of product default and any workbook persistence format belong to
an owner decision following those measurements.
