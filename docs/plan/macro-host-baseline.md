# Guarded VBA corpus baseline

## Workbook admission

The Linux run measures whether DNA Calc can open each workbook and admit its VBA project in
Guarded mode. It uses the `dnacalc recalc` entry point, which opens through CalcHost, prepares
the project in CalcVBA and raises the workbook-open event. The CLI's `macros.ran` field
indicates project admission. Individual event outcomes are held by the session.
[CLI workbook open](../../apps/dnacalc/src/session.rs) ·
[Workbook VBA host](../../crates/calc-host-vba/src/lib.rs)

The wider run uses DNA Calc revision `3cf8c22e`, CalcVBA `943ce551`, Linux x86-64 and
debug binary SHA-256 `e7d9bde9cf95374d03bd5bf343f6e839fad250b0c124e9bc3dfac55a467871bb`.
Each process received a fresh read-only copy whose SHA-256 matched the triage input.
Bubblewrap isolated the namespaces, cleared the environment and mounted the CLI, its
loader and runtime libraries, and that workbook only. Each probe had a 22-second wall limit,
15 CPU-second soft limit and 2 GiB address-space limit. A repository fixture returned
`SimpleNeg(4) = -4` through the Guarded CLI, and a nonexistent procedure was refused.

### Cohort

The 2026-11-14 v5 project census and report-member TSV join on workbook SHA-256.
The TSV identifies 3,368 clean-labelled hashes, and the separately triaged Mendeley set
adds four. The earlier harvested-set triage marks 105 of those GitHub hashes suspect.
Both labels were retained and those hashes were withheld under dc-a3hc. Two other hashes
contain XLM sheets and were withheld under dc-57nx. This leaves 3,265 eligible VBA-only
workbooks in the retained inventory. The host received 3,252. Twelve FUSE hashes and
one unarchived download remain unstaged under dc-oxh8w.1.
[VBA reference census](corpus-vba-census.md) · [Retained triage](vba-corpus-inventory.md) ·
[Member census](corpus-vba-report-members.md) · [Mendeley provenance](corpus-sources.md)

Most staged inputs came from the SHA-keyed `sources/xl-unique/20261116/` store, from
`windows/corpus/`, or from local corpus directories. The workbook base name was taken
from the local path or source inventory where available. R2 corpus objects without
source inventory kept their staged names, and one input had a hash base name.
[SHA-keyed corpus store](corpus-sources.md#cross-corpus-deduplication-2026-11-17-dc-ztvv17)

### Results

The local run admitted 294 of 426 workbooks.
The wider run attempted 3,252 of 3,265 eligible workbooks (99.60%)
and admitted 2,065 projects (63.50% of attempts).
The TSV gives the same counts by source set and file format.
[Workbook admission TSV](macro-host-baseline.tsv)

| Workbook outcome | Count |
| --- | ---: |
| VBA project admitted | 2,065 |
| Class, designer or unbound document module refused | 589 |
| Other project preparation refusal | 216 |
| Project preparation error listing unavailable libraries | 172 |
| VBA project read refused | 105 |
| Process ended on signal | 70 |
| Workbook package refused | 31 |
| Ambiguous document identity | 3 |
| Other CLI host error | 1 |
| **Total attempted** | **3,252** |

The unavailable-library note is attached to a project preparation error by the host.
It can appear beside an independent compile error. Excel parity measurement
remains with dc-oxh8w.6.

## Macro entry points and refusal ranking

The CLI's `run` command invokes a worksheet function by entering a formula into a scratch
cell. Excel macro Subs and workbook mutations require a command context. The workbook VBA
host currently refuses `Application.Run`. Automatic document events have their own
delivery and failure records, which the CLI output does not include. The interpreter exposes
a procedure catalogue and Sub binding, so the missing route is in the workbook host.
[CLI macro invocation](../../apps/dnacalc/src/cells.rs) ·
[Workbook VBA host](../../crates/calc-host-vba/src/lib.rs)

The run recorded zero per-macro outcomes. dc-vba-command-es70w owns invocation and event diagnostics
through the workbook command context. dc-17spk.13 owns provider admission for `Application.Run`.
On that route, dc-oxh8w.1 still needs to run the entries, attribute typed refusals to the
first blocking member or command and count distinct macro entries per blocker.
C API and XLM routes remain with dc-17spk.16 and dc-17spk.19, respectively.

The existing static member census provides source occurrence counts while those runtime
measurements are pending. Excel comparisons for the authored report fixture remain with
dc-piw0.4, and the wider comparison work remains with dc-oxh8w.6.
[Static report-member census](corpus-vba-report-members.md) ·
[Authored report fixture](../../crates/calc-host/tests/fixtures/macro-report/README.md)

## Per-entry run through the command route (2026-11-25, dc-oxh8w.1)

`dnacalc run <book> --all --macro-level guarded` runs every public Sub and Function of the
project through the workbook command route, one freshly opened session per entry, and reports
one outcome per entry: success, VBA run-time error, typed refusal naming the descriptor member
and its owning bead, or not callable. Entries with a required argument or a ParamArray are
counted as `needs-arguments`: the run invents no argument fixtures. Policy denials are recorded
per call whether or not VBA caught the error.
[Command route](../../crates/calc-host-vba/src/command.rs) ·
[CLI](../../apps/dnacalc/src/cells.rs)

**Input.** The corpus store of the 3,265-hash cohort is not on this Linux host, so this run
covers the 4,402 distinct (SHA-256) macro workbooks in the repository: the CalcVBA in-book
corpus, the calc-doc fixtures, the authored report fixture and the engine's Excel calculation
probes. It does not replace the cohort run; it is the first per-entry measurement. DNA Calc at
the dc-oxh8w.1 commit, debug binary SHA-256 `31244f5cc23cc3181918388029c9e97e831fe4ad6ae900150a6b312ab549ad63`,
each probe in a Bubblewrap sandbox (no network, read-only copy, cleared environment, 120 CPU
seconds, 4 GiB address space).

| Workbook outcome | Count |
| --- | ---: |
| Project admitted, entries run | 283 |
| `Workbook_SheetCalculate` (or another Workbook_Sheet* procedure) refuses admission (dc-d7cgp) | 3,897 |
| No VBA project in the file | 110 |
| Workbook package content type refused | 110 |
| Class, designer or unbound document module | 1 |
| Library class not creatable (`New Scripting.Dictionary`) | 1 |

| Entry outcome (283 workbooks) | Count |
| --- | ---: |
| Success (Subs 258, Functions 466) | 724 |
| VBA run-time error (Functions; 91 Object variable not set 516, 424 Object required 274, 5: 2, 7: 1) | 793 |
| Typed refusal (Subs 267, Functions 266) | 533 |
| Needs arguments (Subs 1,040, Functions 1,252) | 2,292 |
| Total entries | 4,342 |

Refusals by missing member, counted by macro entries each would unblock
([TSV](macro-host-ranking.tsv), every entry in [macro-host-entries.tsv](macro-host-entries.tsv)):

| Missing member or command | Owner | Entries | Workbooks |
| --- | --- | ---: | ---: |
| `_Application.Build` | dc-1nbp.5 | 258 | 258 |
| `_Application.Calculation` | dc-1nbp.5 | 258 | 258 |
| `_Application.MacroOptions` | dc-1nbp.5 | 9 | 7 |
| File-project event reads of cached or uncertified cells | dc-snz.15 | 8 | 8 |

The 258 workbooks behind the first two rows are copies of one engine probe project, so the
ranking counts one project twice over; read it as "two members", not 258 independent macros.
No Guarded denial was recorded in this set (no entry asks for files, processes or COM). Run-time
errors 91 and 424 are Functions that read `Application.Caller` or `ThisWorkbook` objects they are
meant to get from a worksheet call; calling them as commands without a caller fails in Excel too.
C API (`xlc*`) and XLM routes are not measured here (dc-17spk.16, dc-17spk.19).

### After workbook-level sheet events (dc-d7cgp, 2026-11-25)

The first pass above refused 3,897 workbooks on `Workbook_SheetCalculate`. Admission now takes
every Excel `Workbook_*` and `Worksheet_*` event the descriptor declares (`WorkbookEvents`,
`DocEvents`) whose arguments are objects or Booleans; the host raises the workbook's
`SheetChange`, `SheetCalculate` and `SheetSelectionChange` after the sheet's own event and
never raises the others (an unraised event does not fire; Excel raises it only for the user
action that causes it). Events with a Long, String or enumeration argument (`SyncEvent`) stay
refused. Same 4,402 workbooks, debug binary SHA-256
`b4d3a32702b5fc77b68004f566bc81d1ad468f8ead2f9fe6be7418ac482f63f6`, same sandbox. The ranking
TSV is regenerated; the entries TSV is the first pass only (the new run has 76,971 rows).

| Workbook outcome | Before | After |
| --- | ---: | ---: |
| Project admitted, entries run | 283 | 4,180 |
| Workbook_Sheet* / AfterSave handler refuses admission | 3,897 | 0 |
| No VBA project in the file | 110 | 110 |
| Workbook package content type refused | 110 | 110 |
| Class, designer or unbound document module; `New Scripting.Dictionary` | 2 | 2 |

| Entry outcome | Before (283 workbooks) | After (4,180 workbooks) |
| --- | ---: | ---: |
| Success | 724 | 16,871 (Subs 4,167, Functions 12,704) |
| VBA run-time error | 793 | 13,214 (91: 8,542; 424: 4,171; 438: 196; 53: 155; 70: 147; 5: 2; 7: 1) |
| Typed refusal | 533 | 11,623 (Subs 4,164, Functions 7,459) |
| Needs arguments | 2,292 | 35,263 (Subs 16,676, Functions 18,587) |
| Total entries | 4,342 | 76,971 |

Run-time errors 53 and 70 are Guarded denials of file and FFI access (155 `Ffi`, 142 `FsRead`
denials recorded). The 4,155 workbooks behind `_Application.Build` are mostly copies of the
engine's probe project; read the ranking as members, not independent macros.

### Argument fixtures (dc-gync0, 2026-11-25)

`dnacalc run <book> --all --fixtures` gives each required scalar parameter a documented value
and runs the entry: Number 1 (Long, Integer, Byte, Single, Double, Currency, Date, Variant),
Boolean True, Text "1". An entry with a ParamArray, an object, array or record parameter, or a
LongLong or Decimal parameter stays `needs-arguments`. Each fixture row says `"arguments": "fixture"`.
The fixture is a probe value, not an Excel answer: a success means the entry ran without a
run-time error for those inputs, nothing more. Measured over the first 1,598 admitted workbooks
of the list (hash order, 29,403 entries; the run was stopped there, the other 2,582 workbooks
are not run with fixtures), same sandbox and binary as above except the Date and Currency
conversion added after this run (3 entries with a Date parameter were `not-callable`).

| Entry outcome | Without fixtures | With fixtures |
| --- | ---: | ---: |
| Success | 6,454 | 13,936 |
| VBA run-time error | 5,043 | 7,330 |
| Typed refusal | 4,437 | 4,867 |
| Needs arguments | 13,469 | 3,267 |
| Not callable | 0 | 3 |

Of the 10,202 entries the fixtures made runnable: 7,482 succeed, 2,287 end in a run-time error
(mostly `array has no bounds`, where a Variant parameter expects an array, and 91/424 object
errors), 430 are typed refusals, 3 not callable.

Still open (dc-gync0): the 3,265-hash cohort per-entry run (needs the SHA store on this host or a Windows
run: `dnacalc run <book> --all --macro-level guarded --json` under the same sandbox, steps in
the lane's RUN-ON-WINDOWS.txt; the owning bead is named in dc-gync0's close note); Application.Run between procedures (dc-80m0f, needs a CalcVBA reentrant dynamic call).

### Fixture run over all admitted local workbooks (dc-oxh8w.30, 2026-11-25)

The fixture run (`dnacalc run <book> --all --fixtures --macro-level guarded --json`, same sandbox)
now covers every admitted local workbook: the first 1,598 (above) and the other 2,582 (1 of them
is not a ZIP archive, so 4,179 of 4,180 ran). Both parts ran before dc-oxh8w.35, so these are the
numbers without the Build and Calculation members.

| Entry outcome | First 1,598 | Other 2,581 | All 4,179 |
| --- | ---: | ---: | ---: |
| Success | 13,936 | 22,547 | 36,483 |
| VBA run-time error | 7,330 | 11,778 | 19,108 |
| Typed refusal | 4,867 | 7,969 | 12,836 |
| Needs arguments | 3,267 | 5,255 | 8,522 |
| Not callable | 3 | 0 | 3 |

**The cohort is 31 projects.** 4,155 of the 4,180 admitted workbooks are copies of the engine's
Excel calculation probes (`crates/calc-engine/tests/fixtures/excel-calc-probes`); grouping the
workbooks by their entry names (probe copies share one project) leaves 31 distinct projects.
Counted once per project the 4,179 workbooks give 374 success, 124 run-time error, 59 refused,
56 needs-arguments entries (after dc-oxh8w.39; 374 / 119 / 61 / 56, plus 3 not callable, before).
Ranking over distinct projects ([TSV](macro-host-ranking.tsv), `projects` and `distinct_project_entries`
beside the per-workbook counts):

| Missing member or command | Owner | Entries | Projects | Entries, every workbook |
| --- | --- | ---: | ---: | ---: |
| `_Application.Build` | dc-oxh8w.35 | 23 | 17 | 4,867 |
| `_Application.Calculation` | dc-oxh8w.35 | 16 | 13 | 4,253 |
| File-project event reads of cached or uncertified cells | dc-oxh8w.38 | 9 | 9 | 2,757 |
| `_Workbook.VBProject` | dc-oxh8w.41 | 9 | 9 | 938 |
| `_Application.MacroOptions` | dc-oxh8w.41 | 3 | 2 | 9 |
| WithEvents subscriptions | - | 1 | 1 | 12 |

Run-time errors 91 and 424 (42 and 40 entries over 17 projects) are probe Functions that read
`Application.Caller` or an `observedSheet` object a prior Workbook_Open sets; Excel fails them the
same way when called as commands, so they are not host gaps.

**After the top two members.** Build and Calculation (dc-oxh8w.35) and the calculation settings
(dc-oxh8w.39) were measured on one workbook per project (31 runs, the new binary each time).
Each step moved the first blocker down the one `Note "profile"/"settings"` line every probe
project prints at start:

| Step | First blockers (entries / projects) | Entries: success / error / refused |
| --- | --- | --- |
| Before | Build 23 / 17, Calculation 16 / 13 | 374 / 119 / 61 |
| dc-oxh8w.35 | CalculationVersion 23 / 17, CalculateBeforeSave 16 / 13 | 374 / 122 / 61 |
| dc-oxh8w.39 | PrecisionAsDisplayed 21 / 15, CalculateBeforeSave (put) 16 / 13 | 374 / 124 / 59 |

Successful entries do not rise yet: those entries read ten settings in one expression, and the
last of them are not implemented (MultiThreadedCalculation needs a Windows descriptor
regeneration, PrecisionAsDisplayed needs calc-doc to load `fullPrecision`, the CalculateBeforeSave
put needs a document setter): all three are dc-oxh8w.40. File-project event reads are dc-oxh8w.38;
`Workbook.VBProject` (the VBIDE object model: VBComponents, CodeModule) and `MacroOptions`
(9 and 3 entries) are dc-oxh8w.41.

## Triage-clean corpus baseline (dc-oxh8w.30, dc-oxh8w.1)

**Input.** The retained v5 member TSV (`dnacalc-corpus/meta/vba-report-members-20261114/`) labels 3,368
hashes clean. Removing the hashes any retained triage marks suspect (`runs/kode-fi/20261112-vba-triage`,
the harv3 census) and the XLM-bearing hashes leaves 3,261 (the 3,265 of the cohort note above differs by
the four Mendeley workbooks and the earlier reconciliation). 2,757 of them are in
`sources/xl-unique/20261116/` and were copied (`rclone copy --files-from`, checksum, no sync or delete) to
`~/corpus-macro/wb` on this host: 1.66 GB. The other 504 are not in the SHA-keyed store (491 from
`windows/corpus/`, 12 FUSE, 1 download) and are unstaged. Nothing was opened in Office or executed outside
the sandbox. Suspect projects were never run.

**Run.** The debug binary built at 06:48 on 2026-11-25 from master after 3087528f1 (SHA-256 `4171b7f66e1f63b0365806c10b99dc9f40dc78c46926e11d8d9bbda3f5924d38`),
`dnacalc run <book> --all --fixtures --macro-level guarded --json`, one fresh bubblewrap per
workbook (read-only copy, no network, 120 CPU s, 4 GiB address space, 300 s wall), eight at a time.

| Workbooks | Count |
| --- | ---: |
| Staged and attempted | 2,757 |
| Opened, project admitted, entries run | 2,125 |
| Project not admitted | 602 |
| No result (timeout or signal) | 30 |

| Entry outcome | Every workbook (2,125) | 315 distinct projects |
| --- | ---: | ---: |
| Success | 412 | 349 |
| VBA run-time error | 353 | 250 |
| Typed refusal | 1,781 | 1,096 |
| Needs arguments | 19 | 18 |
| Not callable | 5 | 5 |
| Failed | 2 | 2 |

A project is the set of entry names and kinds; workbooks that share it count once.

Project admission refusals (602): 251 document project component (dc-hsa5.12), 171 class, designer or
unbound document module (UserForm and Sheet modules not modelled), 91 locked VBA project, 21 invalid Office
project, 20 other project parse, 15 package parts (xlsm content type), 8 legacy stream, 5 binary part,
others below 10.

Refusals ranked by distinct project ([TSV](macro-host-corpus-ranking.tsv)):

| Missing member or command | Owner at run time | Entries | Projects |
| --- | --- | ---: | ---: |
| `Range.Select` | dc-1nbp.5 (closed) | 294 | 88 |
| `_Worksheet.Select` | dc-1nbp.5 | 231 | 43 |
| `Range.AdvancedFilter` | dc-1nbp.5 | 93 | 4 |
| `_Application.ActiveWindow` | dc-1nbp.5 | 87 | 24 |
| `_Application.Goto` | dc-1nbp.5 | 50 | 9 |
| `Range.Range` | dc-1nbp.5 | 38 | 7 |
| `_Worksheet.Unprotect` | dc-1nbp.5 | 28 | 15 |
| `_Worksheet.Visible` | dc-1nbp.5 | 27 | 7 |
| `_Application.Rows` | dc-1nbp.5 | 21 | 13 |
| `_Worksheet.Activate` | dc-1nbp.5 | 20 | 8 |

Run-time errors by project: error 9 `COM status 0x8002000b` 92 entries / 31 projects, error 5 HAL
`CapabilityUnavailable` 32 / 21, error 1004 without text 31 / 6.

The Windows portion of the unstaged set is covered by the addendum below. The refusal batches above
remain with the owners named in the close note of dc-oxh8w.30.

### After dc-oxh8w.46 (selection, navigation, Rows/Columns, Range.Range)

The same 2,757 workbooks, rerun with a binary built after Range.Select, Worksheet.Select/Activate,
Application.Goto (Range, R1C1, A1, name), Worksheet and Application Rows/Columns and Range.Range (SHA-256
`755266761018a39a5559f685948129b01da0e12732bf3ec7b7b767a340ea6d1c`; DisplayAlerts, CutCopyMode and
Range.Activate landed after it and are not measured yet):

| Entry outcome | Before, every workbook | After | Before, 315 projects | After, 312 projects |
| --- | ---: | ---: | ---: | ---: |
| Success | 412 | 771 | 349 | 590 |
| VBA run-time error | 353 | 469 | 250 | 331 |
| Typed refusal | 1,781 | 1,270 | 1,096 | 747 |
| Needs arguments | 19 | 19 | 18 | 18 |

Workbooks that ran: 2,125 before, 2,120 after (five more hit the 300 s limit with the run's CPU shared with a
build). The new run-time errors are entries that used to stop at a refusal and now run into the next gap or an
Excel error. Top refusals now, by distinct project ([TSV](macro-host-corpus-ranking.tsv)): `_Application.ActiveWindow`
256 entries / 46 projects (dc-oxh8w.50), `Range.AdvancedFilter` 96 / 4, `_Worksheet.Unprotect` 38 / 20,
`Range.Sort` OrderCustom 33 / 12 (dc-oxh8w.43), `_Worksheet.Visible` 27 / 7, `Range.End` 23 / 12 (the last four
dc-oxh8w.47 or dc-oxh8w.43).

### Windows corpus files recovered by SHA-256 (ww20)

ww20 recovered all 491 Windows corpus files from local retained copies and verified each copy against
the triage-clean SHA-256 list before and after transfer. No copy was opened in Excel. The remaining
13 hashes belong to the FUSE set (12) and the download set (1) in the member census.

The 491 files ran on dna-firehorse with the same baseline binary (`4171b7f6…4d38`) and Bubblewrap
limits as the original run. Both invocations ran in Guarded mode, with and without `--fixtures`.

| Outcome | Without fixtures | With fixtures |
| --- | ---: | ---: |
| Workbooks attempted | 491 | 491 |
| Project admitted | 28 | 28 |
| Project admission refused | 463 | 463 |
| No result | 0 | 0 |
| Callable entries | 0 | 0 |

The largest refusal group was 210 copies of the W010 Member Oracle form. Tests using the unresolved
`OxVba_TestEventServer` reference accounted for another 28 files. These runs add no entries to the
ranking. Together with the 2,757 attempted earlier, the baseline now covers 3,248 of the 3,261 hashes
in the reconciled cohort. The manifests, per-file receipts and aggregate are in `ww20`.
