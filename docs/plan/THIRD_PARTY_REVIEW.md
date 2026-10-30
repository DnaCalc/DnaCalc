# Third-party code in the published apps: licence review (dc-ysl.2, 2026-10-29)

The workspace is MIT, Copyright (c) 2026 DNA Kode, Inc. Before DNA OneCalc and DNA Calc are
published as compiled wasm, this review lists every piece of code or data in the shipped crates
that came from a third party, with its licence and what a binary release must carry.

**How it was made.** A read-only survey of `crates/` and `apps/` (tests excluded) for code that
says it was ported, translated, adapted or copied from a third party, and of embedded data. The
surveyor also read the legacy provenance records outside the repository. I then checked the main
findings in the source myself: the Bessel family, the x87 chains, the R nmath port, the help
text's origin, and whether the glibc port is reachable. Code ported from this project's own
legacy repositories (OxFunc, OxFml, OxCalc, DnaTreeCalc, DnaOneCalc: MIT, DNA Kode) needs no
notice. Crate dependencies are covered separately: `cargo xtask site pack` writes their licence
texts into THIRD-PARTY-NOTICES.txt from cargo metadata (dc-ysl.3), and
`tools/xtask/licenses.json` already limits them to reviewed permissive licences.

AGENTS.md records the owner's ruling of 2026-10-23 that the third-party numeric code in the
legacy functions may be ported as is. That ruling covers bringing the code into the repository; the
questions below are about distributing it publicly as a compiled binary, which is the owner's to
decide.

## 1. Notices every release carries

`deploy/notices/ported.txt` holds these texts; `site pack` copies it into THIRD-PARTY-NOTICES.txt.

| Source | Where | Licence | Duty in a binary release |
|---|---|---|---|
| AMD AOCL-LibM (log, log10 and their tables) | `calc-func/src/ported/excel_numeric/crt_libm.rs` | BSD-3-Clause, (C) 2008-2022 AMD | Reproduce the copyright, the conditions and the disclaimer (clause 2). The text is in crt_libm.rs's header |
| AMD win-libm (exp, sin, cos and their reductions, sinh, cosh, atan2, hypot; dc-96l, 2026-10-30) | `excel_numeric/crt_libm.rs` | MIT, (c) 2002-2019 AMD | The copyright line and the MIT permission notice |
| Arm optimized-routines `exp`, `log` and their data (re-sourced from the glibc port, dc-96l) | `excel_numeric/mod.rs` (`exp_portable`, `log_portable`), `excel_numeric/tables.rs` | as `pow` below | as `pow` below |
| ExcelFinancialFunctions (Luca Bolognese), bond and odd-period bond routines (dc-96l) | `ported/bond_core_family.rs`, `ported/odd_bond_family.rs` | Apache-2.0 | The licence text, the copyright line and a statement that the files were changed |
| Arm optimized-routines `pow` (pow.c, pow_log_data.c at 0b5e3775) | `excel_numeric/crt_pow.rs` | MIT OR Apache-2.0 WITH LLVM-exception; we take MIT | The copyright line and the MIT permission notice (upstream LICENSE, fetched 2026-10-29) |
| Unicode Character Database 15.0.0 (combining marks table; CaseFolding.txt's simple and full folds, dc-cpr); the CLDR root collation's primary and secondary weights with UCD ages, decomposition types and names (Unicode 15.1, read from the ICU Windows ships, dc-zsn.1) | `calc-fml/src/syntax/combining_marks.rs`, `calc-value/src/case_folding.rs`, `calc-func/src/catalogue/comparison/root_collation.rs` | Unicode License V3 | The Unicode copyright and permission notice |
| fdlibm `e_lgamma_r.c`: six constants (w1 to w6) and the tail's Horner form | `excel_numeric/gammaln.rs` | Sun Microsystems: "provided that this notice is preserved" | Keep the Sun notice. Six constants may not need it; carrying it costs nothing |

## 2. Reimplemented from published mathematics, or a reference only (no notice)

- **Published methods:** Cody-Waite reduction, Tang's tables, Payne-Hanek (`crt_libm.rs`);
  Cody and Hillstrom 1967 and the SPECFUN coefficient values (`gammaln.rs`); the Lanczos g=7
  coefficients (`special_math_common.rs`, `special_dist_family.rs`, `multinomial.rs`); Acklam's
  inverse normal (`normal_log_family.rs`); David Wheeler's YEARFRAC analysis
  (`day_count_common.rs`; no Gnumeric or LibreOffice code); splitmix64 and FNV-1a
  (`calc-engine/src/random.rs`).
- **The Cephes-style series** in `special_math_common.rs` (six lines; Moshier's terms say "may be
  used freely").
- **References only:** glibc in `cosh.rs`, `sinh.rs` and `sqrtpi.rs` (test comments comparing
  results); fdlibm boundaries in `special_dist_family.rs`; "R core" in
  `financial_time_value_family.rs` is a false match ("CR core", correctly rounded).
- **Own data:** Excel's measured outputs (`x87_excel_ground_truth.tsv`, test-only), the
  icons, drawn here (`apps/dna-calc/src/grid/icons.rs`), and the measured departures of Windows'
  text comparison from the CLDR root order (`calc-func/src/catalogue/comparison/windows_collation.rs`,
  dc-zsn.1: about 1,500 facts, each about one code unit, one root weight or one range, measured
  through `LCMapStringEx`; no Windows table is copied, and the order between the departures comes
  from the Unicode data).

## 3. Problems the owner must rule on before the first public binary

**Status 2026-10-30 (dc-96l, licence lane):** row 3's chains are isolated in
`excel_numeric/nt_chains.rs` for the clean-room replacement (dc-96l.3); row 5 is resolved: the
code is AMD's MIT win-libm and Arm's MIT optimized-routines, and what no source says (the
configurations, the fused multiply-adds of sinh, cosh and atan2) was measured against the
runtime; row 8 was wrong (PMT, IPMT and PPMT do reach the port) and is resolved by re-sourcing to
Arm's MIT copy of the same code, checked equal table for table; row 4's text is marked as
Microsoft's in help.rs, with sources in `migration/help-text-sources.tsv`; row 7 carries a credit
to Wichura; row 9's comparison functions are removed (dc-96l.4).

These do not fit an MIT release as they stand. A notice cannot fix the first five. Each is in the
shipped wasm unless the row says otherwise.

| # | What | Where | Source and licence | Shipped | What would settle it |
|---|---|---|---|---|---|
| 1 | Bessel functions | `ported/bessel_convert_family.rs` (BESSELI, BESSELJ, BESSELK, BESSELY) | **Numerical Recipes in C**, proprietary. The code follows NR's routines, not only their mathematics: NR's constants `ACC = 40`, `BIGNO = 1e10`, `BIGNI = 1e-10`, the 9-digit `0.636619772`, NR's variable names and its Miller recurrence (`m = 2*((n+sqrt(ACC*n))/2)`, `jsum`) | Yes | Rewrite from Abramowitz and Stegun 9.4/9.8 and Hart's coefficients with its own structure, keeping Excel's bits (Excel itself follows NR, which is why the port does) |
| 2 | BINOM.DIST and POISSON.DIST mass functions | `ported/discrete_dist_family.rs` (`binom_stirlerr`, `binom_bd0`, the `dbinom_raw` and `dpois_raw` chain) | **R nmath** `stirlerr.c`, `bd0.c`, `dbinom.c`, `dpois.c`, GPL-2+. The tiers and loops follow R line for line; the stirlerr table was regenerated with mpmath | Yes | Reimplement from Loader (2000), the paper, rather than R's code |
| 3 | EXP, LN, LOG10, LOG and POWER's x87 chains | `excel_numeric/x87.rs`, `excel_numeric/mod.rs` (`exp_with` and the chains), run on wasm through `soft87.rs` | Microsoft's CRT `fpw32/tran/i386/87tran.asm`, which the research record (`C:\Temp\ExcelExpFunction\REPORT.md` lines 163 and 189) says was transcribed from **leaked NT source**. No licence | Yes | A legal reading. The Rust re-expresses instruction sequences of about ten steps each; the observable behaviour (the control word 0x133F, the invert path) can also be measured from Excel |
| 4 | Function descriptions | `calc-func/src/help.rs` (about 500 one-line descriptions) | Ported from OxFunc's help seed, whose text matches **Microsoft's support pages** (the OxFunc catalogue's `function_url` column points to support.microsoft.com). No licence | Yes (text) | Rewrite the descriptions in our own words, or drop the table from the published build |
| 5 | Parts of `crt_libm.rs` and `crt_pow.rs` | the runtime-specific exp reduction, Payne-Hanek and hypot, the FMA placement | Read from **Microsoft's ucrtbase.dll** 10.0.26100 machine code | Yes | A legal reading of reverse engineering for interoperability under the licence the DLL came with |
| 6 | Incomplete gamma and beta (`gratio`, `bratio`): GAMMA.DIST, CHISQ, BETA.DIST, T, F, POISSON | `ported/special_math_common.rs`, `normal_log_family.rs` (`branch190_j`) | **ACM TOMS 654 and 708** (DiDonato and Morris, NSWC), "transcribed from the scipy v0.14 cdflib Fortran sources", kept "verbatim" | Yes | Whether ACM's algorithm policy (no distribution for direct commercial advantage) applies. The authors were US Navy employees and scipy lists cdflib as public domain; if that holds, a notice naming the ACM publications is enough |
| 7 | Inverse normal tails | `normal_log_family.rs` (AS 241 tables C to F) | **Applied Statistics AS 241** (Wichura 1988), Royal Statistical Society; StatLib distributes it "provided that no fee is charged" | Yes | The published coefficients are mathematics; whether the short routine needs the RSS terms |
| 8 | glibc 2.29 exp and log | `excel_numeric/mod.rs` (`exp_portable`, `log_portable`, `specialcase`, `excel_expm1`, `excel_log1p`), `excel_numeric/tables.rs` | glibc, LGPL-2.1+ | **No.** Only tests call them (`mod.rs` 1259-1263; `log_general_hi_lo` is used only by `log_portable` and `excel_log1p`), so the release wasm drops them | Move them under `#[cfg(test)]` or delete them; or attribute the same author's tables to Arm's MIT copy (`math/exp_data.c`, `math/log_data.c`) after comparing |
| 9 | Numerical Recipes `gser`/`gcf` | `special_math_common.rs` (`old_nr_gamma_p`, `old_nr_gamma_q`) | Numerical Recipes | No (inside `#[cfg(test)]`) | Delete the comparison functions |

Until the owner rules on rows 1 to 7, `site pack` builds and the local checks run, but the release
must not be published. `OWNER_STEPS.md` (dc-ysl.5) says so as its first step.
