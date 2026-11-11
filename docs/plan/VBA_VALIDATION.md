# VBA validation: what exists, what ran, the corpus and the Linux run (2026-11-11)

Bead: "VBA validation corpus and Linux runner". Companion to [VBA_UDFS.md](VBA_UDFS.md). Pass = matched Excel;
fail = mismatched + refused + missing, as counts over all cases, per route and per backend. A pooled number
over routes would say nothing, so none is given.

## 1. What existed

| Evidence | Where | Own or third party | Compared with Excel how | State |
|---|---|---|---|---|
| DnaSimple, 109 cases | `crates/calc-xll-reference/fixture/vba/DnaSimple.bas`, `simple-udf-cases.json`, `traces/2026-11-11-simple-udfs/` | own, MIT | formulas in H1 of a new workbook with the .xlam open, and again inside DnaSimple.xlsm (same 109 answers); raw rows with types in `excel/simple-xlam.cases.json` | OneCalc desktop 109/109, DNA Calc desktop file route 0/109 (README of the trace) |
| DnaExamples, 26 cases (+1 module edit) | same folder, `x9-vba-cases.json` | own, MIT | same, with the .xlam open | OneCalc 26/26 (27/27 as .xlsm), DNA Calc .xlam 23/26 (3 object-model refusals), .xlsm 0/26 |
| active-objects, 56 rows | `fixture/vba/active-objects-measured.json` | own | 4 scenarios of ActiveSheet / Selection / Caller | dc-0f1k.12 (OneCalc) |
| Recalculation observations | `crates/calc-vba/tests/fixtures/excel-recalculation/` (17 stages, 23 cells) | own | Excel in safe mode (normal start crashed in TestCrashGuard) | no Rust file reads it |
| VBA UDF names in files | `crates/calc-doc/tests/fixtures/vba_udf_names/` (18 packages, .bas sources) | own | file-format and name-resolution study (dc-snz.7), not a code corpus | calc-doc tests |
| P01-P22 probes | `crates/calc-engine/tests/fixtures/excel-calc-probes/` (about 4,170 .xlsm) | own | calculation-chain probes; macros are the carrier, not the subject | calc-engine |
| OxVba oracle probes | `C:\Work\DnaCalc\OxVba\oracle\` (probes.bas, probes_controlflow.bas, `results/*.json`) | own, MIT | `run_oracle.ps1` via Application.Run | 56 answers; **seeded below, re-captured through cells** |
| OxVba conformance | `OxVba\conformance\tests\*.bas` (534 files, 192 in the manifest), `golden/values.csv` | own, MIT | none: the 192 golden rows are the VM's own values (self-golden), never a pass | candidate for Excel capture (section 6) |
| OxVba Excel measurements | `OxVba\conformance\excel-measurements\` (library tranche, numeric-mode matrix, double-string, declaration limits: `.vba` + `.excel.txt`) | own, MIT | Excel answers per probe line, with `known-differences` lists | candidate for conversion (section 6) |
| External watchlist corpus | `OxVba\.external\vba-corpus\` (riff, wasabi, VBA-Web; gitignored) and `.external\chibipdf`, `.external\sqliteforexcel` | third party, MIT (each has a LICENSE) | none against Excel: run only against OxVba's compiler and VM as admission / smoke (VW-01..23, RF-01..12 in the removed `docs/EXTERNAL_VBA_CORPUS.md`, commit 78e386cf^) | local copies; riff, wasabi and VBA-Web seeded below |

The earlier "downloaded projects running as comparison" were the three watchlist projects. They were compared with
OxVba's own VM and with `Declare`/COM host probes, never with Excel answers; their value was the
compiler and runtime defects they exposed. The old oracle script (`run_oracle.ps1`) stops every Excel process and
writes `HKCU` Excel security keys; it is not ported. Only the probe sources were kept.

## 2. The corpus (new, in the repository)

`crates/calc-vba/tests/corpus/<id>/project.json` plus the files it names. A project holds:
`tier` (W = a worksheet call of a project function; L language and S sequence tiers are not populated yet),
`origin` (class own or third-party, licence, copyright, URL and commit, where the local copy came from),
`modules` (path, kind, sha256 of each source as copied; git may normalise line endings, so treat the hash as provenance, not a check), `references`, `needs` (Declare, late binding, class modules,
com:Scripting, ActiveSheet/Selection, Excel object model; computed from the source), `cases` (a JSON list
`[{formula, value}]`, the format `simple-udf-cases.json` already uses) and `excel` (build, route, environment, the
raw rows with types, the private-desktop guard). `baseline.json` holds the reviewed counts. Own fixtures are
referenced in place (`../../../../calc-xll-reference/fixture/vba/...`), not copied.

| Project | Origin and licence | Modules | Cases | Excel answers |
|---|---|---|---|---|
| `dna-simple` | own, MIT | DnaSimple.bas (+ DnaSimple.xlsm for the file route) | 109 | existing (2026-11-11) |
| `dna-examples` | own, MIT | DnaExamples.bas, DnaCounter.cls (+ .xlsm) | 26 | existing (2026-11-11) |
| `oxvba-error-handling` | own, MIT (OxVba oracle/probes.bas, non-ASCII punctuation in comments made ASCII) | 1 | 33 | **captured today through cells** |
| `oxvba-controlflow` | own, MIT (oracle/probes_controlflow.bas) | 1 | 23 | captured today |
| `vba-web` | VBA-Web v4.1.6, MIT, (c) 2016-2019 Tim Hall, commit 9dbcc751 | `src/` verbatim: WebHelpers.bas + 4 classes + interface (192 KB) | 45 | captured today (all six imported, Scripting reference) |
| `vba-web-excerpt` | same, verbatim whole functions and enums of WebHelpers | 1 | same 45 | captured today; equals the full project 45 of 45 |
| `vba-json` | VBA-JSON, MIT, (c) 2016-2019 Tim Hall, commit 06b6c45a (clone 212 KB), with VBA-Dictionary (below) verbatim; own wrapper `JsonProbe.bas` | 3 | 21 | captured today |
| `vba-dictionary` | VBA-Dictionary, MIT, (c) 2020 Tim Hall, commit 757aea92 (clone 171 KB); own wrapper `DictProbe.bas` | 2 | 15 | captured today |
| `riff` | uesleibros/riff, MIT, (c) 2026 Ueslei Paim, commit cafe9bc0 | Riff.bas (159 KB) | admission only | none |
| `wasabi` | uesleibros/wasabi, MIT, commit 36afb522 | Wasabi.bas (371 KB) | admission only | none |

VBA-Web, riff and wasabi came from local copies in OxVba's `.external`; VBA-JSON and VBA-Dictionary were downloaded today (shallow clones, owner-approved, commits above). Third-party code is verbatim with
its LICENSE file beside it.

Excel answers were gathered with a new `-Action project` of `fixture/vba/vba-job.ps1` (extends the existing job; no
second harness): a fresh workbook gets the modules through the VBE object model and the Scripting reference, each
formula is entered in H1 as Formula2, calculated, `Value2` recorded; nothing is predicted. Excel 16.0.20527 in the
owned hidden session on the private desktop; every `excel/guard.json` shows 0 windows and 0 foreground hits on the
owner's desktop; `AutomationSecurity` was 1 and the job changed no Trust Center, macro-security or registry
value. Third-party workbooks were not opened: VBA-Web source was imported into our own fresh workbook, and only
Obfuscate, UrlEncode, UrlDecode, JoinUrl and MethodToName (read beforehand; pure string code) were called.
Recorded environment gaps: safe mode and EnableEvents were not recorded by this job; bitness and locale are
64-bit en-US only.

Types: `excel/rows.json` keeps `{value, type}` and the displayed text for each row. The older answer files lose
type information: `simple-udf-cases.json` has no types, so `=SimpleDate()` is `46337.0` and `SimpleMoney()` a bare
number (their Date and Currency type and the cell format are only in the raw rows' absence); array results are
recorded as the first element, and the array cases were checked through `INDEX`/`SUM`/`ROWS`/`COLUMNS`
(`SimpleSeq`, `SimpleRow`, `SimpleGrid`, `SimpleSplit`). Re-capture those with types before scoring dates, currency and arrays.
Not done: conversion into `dnacalc-capture-record` entries and a `dnacalc-trusted-evidence-code` manifest
(`BULK_PORT_PLAN.md`), so the file route of `cargo xtask corpus --trusted-code` does not yet run this corpus.

## 3. The Linux runner

`apps/dna-onecalc/tests/vba_corpus.rs`:
`cargo test -p dna-onecalc --test vba_corpus -- --nocapture` (on firehorse, no Excel, no Windows). Per project it runs:

- **module route**: OneCalc's "VBA functions" route (`Session::load_vba_module` equivalent), once per explicit backend,
  interpreter and JIT (`calc_vba::load_module_with_backend`; a JIT decline is a refusal, no fallback);
- **xlsm route**: OneCalc's file route (`file::open`, `open_project`, `enable_macros`) on the project's own `.xlsm`,
  engine-chosen backend.

It does **not** run DNA Calc's file route (`calc-host-vba`, the route DNA Calc users get; 0 of 109 on Windows
per the trace README) nor `cargo xtask corpus --trusted-code`. A pass here says nothing about those routes.
`DNACALC_VBA_CORPUS_REPORT=<file>` writes the per-case report; `DNACALC_VBA_CORPUS_UPDATE=1` rewrites
`baseline.json`; the test fails when any count moves.

## 4. Results (firehorse, calc-vba pin 18d06139)

| Route | pass | mismatched | refused | missing | cases |
|---|---|---|---|---|---|
| module / interpreter | 74 | 27 | 216 | 0 | 317 |
| module / JIT | 47 | 9 | 261 | 0 | 317 |
| xlsm (OneCalc file route) | 109 | 0 | 26 | 0 | 135 |

Per project (matched / mismatched / refused):

| Project | module interp | module JIT | xlsm |
|---|---|---|---|
| dna-simple (109) | 0 / 0 / 109 | 0 / 0 / 109 | **109 / 0 / 0** |
| dna-examples (26) | 0 / 0 / 26 | 0 / 0 / 26 | 0 / 0 / 26 |
| oxvba-controlflow (23) | 23 / 0 / 0 | 23 / 0 / 0 | n/a |
| oxvba-error-handling (33) | 24 / 9 / 0 | 24 / 9 / 0 | n/a |
| vba-web-excerpt (45) | 27 / 18 / 0 | 0 / 0 / 45 | n/a |
| vba-web (45) | 0 / 0 / 45 | 0 / 0 / 45 | n/a |
| vba-json (21), vba-dictionary (15) | 0 / 0 / 36 | 0 / 0 / 36 | n/a |
| riff, wasabi (admission) | riff compiles; wasabi does not | both decline | n/a |

Refusals and mismatches by reason (each reproduced with a minimal snippet):

1. **Standalone module has no Excel constants**: `CVErr(xlErrDiv0)` is "Variable not defined", so the whole
   DnaSimple.bas is refused on the module route; the same project in a `.xlsm` runs 109/109. (Module route
   needs the `XlCVError` constants the office route declares.)
2. **Optional parameter typed with a module Enum and any explicit default** (`Optional m As E = E.A`, even `= 0`):
   raises VBA error 91 when omitted; with no default or an Long type it works. Causes the 18 vba-web-excerpt
   mismatches (UrlEncode, UrlDecode). Minimal repro: Enum E, `Function F(Optional m As E = 0) As Long: F = m`.
3. **Single-line `For i = 1 To 3: s = s + i: Next i`** does not parse ("expected Next"); stops Wasabi.bas (its
   opcode builders use it) and any code using the idiom.
4. **`Err.Source` project name**: CalcVBA answers `UdfProject`, Excel `VBAProject` (8 of the 9 error-handling
   mismatches). The ninth, `PROBE_oe_goto_minus1`, is `GUARD` in Excel's cell but the full `H1(11);...` trace in the
   old Application.Run oracle and in CalcVBA: Excel's answer for this probe differs when the function is called
   from a cell, so the Application.Run answers cannot stand in for cell answers.
5. **Multi-module and class projects**: the module route loads one module, so vba-json (JsonConverter calls the Dictionary class) and vba-dictionary (a class) are refused ('Variable not defined', 'New dictionary'). They are pure VBA with no Declare/COM on the paths called, so they are the first candidates for a source-level multi-module loader. Also:
   **Project classes and COM**: DnaExamples (`New DnaCounter` class on the single-module route; on the file route
   `Scripting.Dictionary` and the Scripting reference "cannot be resolved on this platform"), VBA-Web (`New Dictionary`).
   These are the expected Linux refusals for `needs: com:Scripting` / class modules; they are typed and asserted
   by the baseline, not silent.
6. **JIT admission**: the JIT declines UrlEncode (a `CallExtern` of InStr/Join-family intrinsics, "M4-4") and Riff's
   native `Declare` calls; with no fallback that is 27 more refusals than the interpreter on the module route.
   Interpreter and JIT disagree on no case that both run (oxvba probes identical).

## 5. Gaps

- Tier L (single procedure, VarType and Err in the answer) and tier S (sequences, events) are not populated; the
  192 conformance rows have no Excel answers; `excel-recalculation` (23 cells) is still read by no test.
- DNA Calc's file route and `--trusted-code` are not run on this corpus; dna-simple on that route is 0 of 109.
- Types lost in `simple-udf-cases.json` (SimpleDate, SimpleMoney, array results); Excel environment partly unrecorded.
- Multi-module source projects have no source-level loader (the module route loads one primary module); classes
  need a `vbaProject.bin`, which Excel builds on Windows.
- Third-party code executed (coordinator ruling: open-source MIT libraries whose code was read, pure functions only, own fresh workbook and Linux, are fine; VBA from public spreadsheet corpora is not): VBA-Web helpers, VBA-JSON and VBA-Dictionary (the Windows-only and Mac `Declare`/`popen` paths of VBA-JSON's UTC helpers are not called). Riff and wasabi were only compiled.

## 6. Further projects

| Project | URL | Licence | State |
|---|---|---|---|
| VBA-Dictionary, VBA-JSON | https://github.com/VBA-tools/VBA-Dictionary, https://github.com/VBA-tools/VBA-JSON | MIT | seeded (section 2) |
| stdVBA | https://github.com/sancarn/stdVBA, commit 963efe5b82dcd29ad553af1d78ad89ed3a49d6d4 | MIT, (c) 2021 James Warren | cloned to a scratch directory only (20.9 MB with .xlsm files, class-heavy); not seeded, nothing run, no workbook opened. Needs the multi-module loader first |
| VBA-Expressions | (VBA-tools/VBA-Expressions does not exist) | n/a | dropped from the list |
| Awesome VBA (index) | https://github.com/sancarn/awesome-vba | index | source of more candidates |
| local, unseeded | OxVba `.external\chibipdf` (56 KB, MIT), `.external\sqliteforexcel` (3 MB, owner's fork) | MIT | Declare-heavy; compile-only |
| own, unconverted | OxVba conformance (192 manifest rows, 534 files) and `excel-measurements` (library tranche 177 KB .vba, numeric-mode matrix 920 KB) | MIT | tier L with Excel answers needing conversion or capture |
