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

Still open (dc-gync0): the 3,265-hash cohort per-entry run (needs the SHA store on this host or a Windows
run: `dnacalc run <book> --all --macro-level guarded --json` under the same sandbox, steps in
the lane's RUN-ON-WINDOWS.txt), argument fixtures for the 2,292 `needs-arguments` entries,
`Workbook_Sheet*` event admission (dc-d7cgp), Application.Run between procedures (dc-80m0f).
