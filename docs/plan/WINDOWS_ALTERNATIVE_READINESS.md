# Windows Excel-alternative readiness

2026-11-13 · dc-7x3z · astra1 / astra1b · revised after Opus review, not a release approval.

DNA Calc has useful workbook calculation, editing, preservation, VBA and native add-in
machinery. The evidence does **not** yet support replacing Excel for an arbitrary Windows
user's working files. The first release should describe the workflows it actually demonstrates:
open a named workbook, enable its approved code and providers, edit, calculate, undo, save,
reopen in Excel and print. Success at loading a package or calling an isolated function is
insufficient evidence for that whole sequence.

This assessment ranks remaining gaps by owner priority, damage to real work and prerequisite
value, informed by reach. The rank numbers are unchanged after review: ranks 2–3 precede
drawings at rank 5 because the owner prioritizes working VBA/XLL, **not because more users meet
them first**. Atlas counts 6,851 drawing-bearing files, versus 752 with VBA and 174 with `_xll.`
calls; by that measure drawings have much wider reach. An installation is chronologically
first; a trustworthy edit/save cycle and the correct workbook behind an add-in are engineering
prerequisites. Encrypted-file admission (rank 9) can likewise be a user's first blocker.
For a user whose business depends on macros, rank 3 becomes an immediate blocker; for a
paper-reporting workflow, rank 6 does. These are judgments, not measured user-population shares.

## Evidence and its limits

The original read was at `cf08c5b3`, with tracker pulls through `8184fe1f`. Review corrections
were checked against master `9ea13fcc`, including later bead comments and the source assertions.
The final pull to `24f38a8b` added Windows VBA corpus overrides; rank 3 includes their recorded
counts, summed from the updated baseline without executing the corpus.
No corpus, bank, Excel, browser or Windows execution was rerun for this note. Test names below
identify assertions read, not fresh passing runs. Recorded results
retain their original revision and route. `br ready`, the complete `br list`, and the cited
bead records were read; “open” below includes `in_progress`.

The **“Goals, in order” in [the week plan](WEEK_PLAN_2026-11-13.md) are the owner's answers**,
agreed on 2026-11-12. They call for the Windows alternative with working VBA/XLL and portable
engine/document code; the high-priority Linux CLI/library runner (Goal 1a, dc-icq4); owner-signed
installers; the web site (Goal 3, dc-6q2k); CalcVBA samples at 100% and performance measured
against Windows VBA (Goal 4, dc-tey4); and publishing the basic system, then improving it
continuously (Goal 5). Versioning/streams precede preview milestones and each publish needs
the owner's yes. Bit-exactness continues with a smaller share.

The owner's 2026-11-13 comment on dc-7x3z records the subsequent CalcVBA decision: writes were
held, then **all CalcVBA work and its coordination moved to the separate CalcVBA session**.
Goal 4 and dc-tey4 still stand; they are not waiting for a new agreement. This lane only
documents their product dependency and changes no runtime code.

### Three populations that must not be combined

| Population | Figure read | Meaning |
|---|---|---|
| Atlas | 20,672 unique packages; 25,757,255 formula cells | Feature inventory, not passes |
| Enron reader follow-up | 15,871 XLSX files open; 58 OLE2 remain | Host admission, not calculation |
| Full bank at `5ed48847` | 4,607,949 / 4,729,476 matched; 121,527 nonmatches | 97.4304% of recorded function rows |

The [Corpus Atlas](../fun/corpus-atlas.html), embedded `ATLAS` object, has 10,332 files with
formulas. It counts 6,851 drawing-bearing files, 2,539 with charts, 752 with VBA parts,
174 with `_xll.` calls, 2,910 with external-link parts and 14,248 with printer settings.
Its [aggregator](../fun/tools/corpus_atlas_aggregate.py) excludes `repo`/`stage`, deduplicates
hashes and admits `package.opened`, not `reader.opened`. It omits unreadable containers and can
include host-load refusals. Part presence is not proof a feature is used.

The [classification](corpus-classification.md), [classifier](../../tools/xtask-harness/src/classify.rs)
(`classify_shared`, `classification_reports_the_reader_repair_kinds_and_counts`) and
[reader tests](../../crates/calc-doc/tests/reader_tolerance.rs) separate package facts from
host outcomes. The table above uses the recorded dc-o41m follow-up. The wider classification
finds 5,461 OLE2 files across EUSES/libraries/Enron. Its older table and “does not understand”
list are not current defects: later paragraphs and tests supersede the duplicate-ID and
missing-relationship refusals. Its 20.7 million Enron formula cells are inventory, not matches.

The full-bank figure comes from **dc-ymjf.2.18's close record at `5ed48847`** and dc-ymjf.2.2's
notes; nonmatches are derived by subtraction. TEXT is 2,820/2,861 there. The same close records
oracle corpus 77,036/78,264, hence 1,228 inclusive failures. Neither population is an arbitrary
workbook or Windows desktop success rate. I did not inspect the full raw bank payload or
invent a mismatch/refusal breakdown. Atlas's embedded bank standings are older (TEXT 7/2,861),
so use Atlas for frequency, not current exactness.

The [Enron saved-cache report](enron-saved-cache-triage.md), at `8081154a` plus tracker pulls,
records 12,961,186 saved-value agreements out of 19,666,149 compared formula cells, and **zero
live passes**. Its 1,921,404 link-cache agreements are separate, not calculated passes. Its
older function-defect list is also historical. These figures locate work; they cannot certify
today's results. All acceptance below retains refusals, missing observations and failed files
in the failure population. A saved, stale or held value never becomes a calculated match.
The acceptance thresholds below are proposed completion criteria, not achieved results or
new owner-approved performance budgets. Named sets bound a claim; passing them alone cannot
establish compatibility with arbitrary workbooks. Missing files/observations stay pending or
failed, and an expected refusal tests safe behavior without becoming a compatibility pass.

## Ranked gaps

### 1. A workbook must survive the user's edit, undo and save sequence

**What the user sees.** Opening can succeed while structural edits remain unavailable. A row
insertion on an ordinary sheet can be refused because another sheet is preserved; removing a
sheet owning charts or tables can also refuse. Recent Undo and save improvements reduce the
risk, but their remaining Excel observations matter especially to a manual-calculation workbook
being handed back to a colleague.

**Evidence.** [Host edit-time tests](../../crates/calc-host/tests/dc_1nbp_12_edit_time_refusals.rs)
assert the refusal, unchanged revision and successful unrelated cell save for macro,
chart/dialog and partless sheets. This is an improvement: **dc-1nbp.12 is closed**, not a
remaining late-save trap for those tested operations. [Reader tolerance tests](../../crates/calc-doc/tests/reader_tolerance.rs)
(`reader_tolerance__an_edited_save_writes_excels_repairs`) cover eight repaired fixtures;
**dc-1nbp.13 is closed**. The untouched-save test deliberately keeps original bytes pending
Excel experiment E7(a). **Open beads .14–.17 already have implementation at `03270d31`:**
[host round-trip tests](../../crates/calc-host/tests/round_trip.rs), `dc_1nbp_14__*`,
`dc_1nbp_15__*` and `dc_1nbp_17__*`, cover workbook `calcOnSave`, stale results through Undo/table
rebuild, and chart/rule/table reference rewrites. For **dc-1nbp.16**, the same file's
`contract_p07_edits_moves_undo_reload_cancel_and_reopen_reject_late_results` includes inserted
rows, added sheets and table changes. Its companion
`contract_p07_insert_row_then_formula_at_old_address_rejects_the_old_token` asserts rejection,
unchanged revision, the replacement `=777` value and zero resumed suffix calls, then rejection
again after Undo. [The worker](../../apps/dna-calc-worker/src/server.rs),
`dc_1nbp_14__skin_save_uses_the_workbooks_calculate_before_save_in_manual_mode`, verifies the
page's Save uses the workbook setting. The convenience HostSession save's `false` is therefore
**not** the page's current behavior. [EngineState](../../crates/calc-engine/src/state.rs)
implements `rebuilt_holding_results`, used by host restore and table rebuild. Remaining evidence
includes Excel's chart `#REF!` spelling, post-Undo UDF counts and multi-workbook save-setting
behavior (E6/E12/E13). The sheet-removal test still refuses an object-owning sheet. I found no
proof that every imported object survives every supported structural edit.

**Acceptance observable.** Through the Windows page, start with
[D2 structural-base.xlsx and the three Ofgem workbooks named by source hash in ofgem-round-trip.json](../../crates/calc-host/tests/fixtures/d2),
the eight repaired inputs in `reader_tolerance__an_edited_save_writes_excels_repairs`, the
macro/chart/dialog/partless inputs in `dc_1nbp_12_edit_time_refusals.rs`, and the XLSM sets in
[m5-acceptance.json](../../crates/calc-xll-reference/fixture/m5-acceptance.json).
Use the existing fixtures and registered E6/E7/E12/E13 probes: edit cells; insert/undo rows;
rename sheets; save in Automatic and Manual; reopen in Excel. Record formula/value provenance,
preserved parts, repair messages and UDF/event counts. Each advertised operation must preserve
the workbook and match the corresponding Excel observation. Unsupported operations must refuse
before changing it. Retained display caches after Undo must remain labelled stale. Determine
untouched-save and CalculateBeforeSave behavior from E7/E12 and the other registered probes,
not from assumptions about Excel. Completion for this set requires **100% of advertised
operation/Excel comparisons**, zero unexpected repairs on reopen, zero lost preserved parts,
zero stale-token publications and exact observed UDF/event counts. Unsupported-operation
controls must all refuse with unchanged state. Missing live formula answers prevent the
calculation portion from passing; preserved/manual caches retain their separate status.

**Open coverage.** dc-1nbp.14–.19; dc-kapl.2.3; dc-8wp.16 for pivot structural edits;
dc-1nbp.22 for view-manifest/name round trips. No single bead proves the complete interactive
open/edit/undo/save/reopen sequence above. Extend the existing round-trip and desktop suites.

### 2. XLL and COM add-ins do not yet see the interactive workbook faithfully

**What the user sees.** A numerical XLL can load and answer while workbook-aware functions,
Range/Caller access or event-driven commands fail. An add-in can be told “Book1” instead of
the file the user opened. This can block an entire business workbook despite a strong scalar
function score.

**Evidence.** [Desktop app composition](../../apps/dna-calc-desktop/src/app.rs) passes the
constant `WORKBOOK = "Book1"` and `active_sheet: None` to `AddinHost::new`; its interactive path
does not call `sheet_changed` or `after_calculate`. [The engine harness](../../apps/dna-calc-desktop/src/engine.rs)
explicitly binds a sheet and delivers those notifications. [Desktop XLL tests](../../apps/dna-calc-desktop/tests/xll.rs)
are Windows-only `--engine` tests: they cover native async continuation, late-return rejection,
status 64, pool callbacks and an XLSM UDF alongside an XLL. They do not prove page integration.

The committed [X13 acceptance records](../../crates/calc-xll-reference/traces/2026-11-10-x13-boundary/README.md)
and `m4-acceptance/summary-{x1,other-sets}.json` show **350/353** through the **OneCalc page**
at the recorded `a967db01` build: X1 289/289, other sets 61/64; the three remaining cases
are mismatches in `calcvba-udf`, not refusals. This includes Excel-DNA, xlw and xlOil examples;
it is not certification of arbitrary COM add-ins. [ABI tests](../../crates/calc-xll-abi/src/lib.rs)
check layout/status constants; [COM tests](../../crates/calc-com-host/src/projection.rs) cover
event cookies, retained objects, controls and ribbon reentry. [AddinHost tests](../../crates/calc-addin-host/src/host.rs)
cover one callable table and restart. Its `NoWorkbookReads` explicitly refuses add-in workbook
event cell reads under **dc-0f1k.4**. These are substantial foundations with a still-missing
application connection.

Office COM add-ins and their UI are a separate limitation. Atlas counts **29 packages with
`customUI` parts**, not 29 proven COM add-ins.
[projection-office.h](../../crates/calc-com-host/src/projection-office.h) already
discovers private HKCU registrations whose server module the host owns, activates the real
interface with `CoCreateInstance`, and calls `OnConnection`/`OnDisconnection`.
`read_ribbon_ui` calls `IRibbonExtensibility::GetCustomUI`; `AddinHost::ribbons` exposes the
returned XML. This bounded Excel-DNA route is not a general Office COM add-in loader or a
rendered, clickable ribbon. [Hosting status, section 20](../architecture/XLL_COM_HOSTING.md#20-excel-dna-scalar-and-private-com-support-dc-snz2)
states the same limit. A business add-in's ribbon commands can therefore be unavailable even
when its native UDFs work; retaining workbook `customUI` XML is not executing those commands.

**Acceptance observable.** Open a named, multi-sheet workbook in the DNA Calc page and run the
existing Excel-DNA/xlOil cases there. Observe correct Name/FullName/Path, active sheet,
Caller/ThisCell/Range/Union, rectangular and multi-area SheetChange, AfterCalculate counts and
EnableEvents behavior. Exercise Save As, sheet changes, unload/reload, queued macros, native
async and RTD while editing. Compare values, ordered callbacks and object lifetimes separately
with the retained Excel traces; a successful `--engine` run alone does not close this gap.
For Office UI, extend the existing `x8-ribbon-cases.json`/Excel-DNA ribbon example through the
DNA Calc page: connect, render controls, invoke their actual callbacks, disconnect and reload,
with matching effects and lifecycle traces. Keep this distinct from workbook `customUI` and
from importing an arbitrary installed Office add-in; neither is established by XML inspection.

**Open coverage.** dc-1nbp.3 first, dc-0f1k.24, dc-1nbp.4–.6; dc-6332.10–.12 and .14;
dc-snz.23 and dc-0f1k.4. Broader COM identity/apartment/no-Office evidence: dc-6332.16.
No dedicated open bead was identified for general Office add-in discovery/loading or a complete
interactive ribbon; the existing bounded host should be extended. These beads overlap
intentionally: one session bridge and event source should serve them.

### 3. Real VBA projects remain much broader than the passing samples

**What the user sees.** Simple macro functions can work; a project with classes, library
behavior, object-model calls or workbook events can refuse or return a different result.
Running a standard module alone is not the same as opening the workbook containing it.

**Evidence.** I summed [baseline.json](../../crates/calc-vba/tests/corpus/baseline.json) at
`24f38a8b` without
executing or updating it. The [runner](../../apps/dna-onecalc/tests/vba_corpus.rs) defines the
routes and denominator; its XLSM route is **OneCalc's file session**, not the DNA Calc page.

| Recorded route (common unless labelled Windows) | Matched | Mismatched | Refused | Missing Excel answer | Total |
|---|---:|---:|---:|---:|---:|
| XLSM (two workbooks) | 131 | 0 | 4 | 0 | 135 |
| Project / interpreter | 163 | 3 | 151 | 77 | 394 |
| Project / JIT | 163 | 3 | 151 | 77 | 394 |
| Module / interpreter | 209 | 1 | 317 | 77 | 604 |
| Module / JIT | 209 | 1 | 317 | 77 | 604 |
| Windows project / interpreter | 223 | 15 | 151 | 5 | 394 |
| Windows project / JIT | 163 | 3 | 223 | 5 | 394 |
| Windows module / interpreter | 209 | 1 | 389 | 5 | 604 |
| Windows module / JIT | 209 | 1 | 389 | 5 | 604 |

These routes overlap; do not sum them into distinct tested programs. The XLSM rows are
dna-simple 109/109 and dna-examples 22/26, unchanged by the Windows overrides. **dc-tey4.1 is
closed** with a recorded Windows run matching those figures; its overall runner was 6 passed,
1 failed because other rows came from an older binary. Therefore neither “Windows refuses all
135” nor “the entire Windows corpus is green” follows. Project routes have dictionary 15/15,
JSON 21/21, stringbuilder 35/35 and keyedcollection 23/24; arraytools, regex and vba-test are
refused. Riff/Wasabi account for 77 missing-answer rows in the common baseline. The final pull's
Windows overrides record Riff project/interpreter 19 matched and five missing, and Wasabi
41 matched and 12 mismatched. Their module/JIT routes still refuse the newly answered cases;
the five missing Riff answers remain failures. These records supersede the original empty
Windows override, not the unchanged common baseline. The full VBA-Web project's 45 sample
answers do not establish its network/COM workflows.

**Acceptance observable.** The separate CalcVBA session owns the agreed Goal 4 work: samples reach
100% against recorded Excel answers on both backends, with missing answers acquired rather
than counted away. Then the DNA Calc Windows page opens the same XLSM files, runs their UDFs
and automatic events against the live sheet, edits/recalculates, saves and reopens them with
matching results and event traces. Keep module, project and file results separately visible.
Include the existing long-lived Range regression in an Excel-observed workbook.

**Open coverage.** dc-tey4, .5 and .6; dc-hsa5.22/.23, dc-hsa5.12, dc-1nbp.7,
dc-snz.5/.6/.9/.10/.15; dc-90ro for the unresolved WithEvents COM type. dc-yqq0 and its open
children own Automation representation work. The 4,096 released-Range lifetime defect is
already fixed by dc-1nbp.2; its in-workbook Excel witness remains unverified in that close note.

### 4. Recalculated answers, links and analytical refresh need workbook evidence

**What the user sees.** Results can differ after recalculation even when a file initially looks
right from saved values. A missing provider, link source or analytical service can prevent a
model from updating. A high bank score does not explain whether that user's dependent cells
were freshly computed.

**Evidence.** The bank's 121,527 nonmatches remain real failures under the required bit-match
standard. Among the functions relevant to this triage, Atlas file counts are IF 2,216,
VLOOKUP 936, ROUND 716, SUBTOTAL 485, SUMIF 398, CELL 355, CONCATENATE 298, TEXT 252 and COUNTIF 211.
This is not the overall frequency top nine (SUM alone appears in 7,118 files), nor is it the
week plan's fix3 assignment list. Frequency indicates reach; the stale Atlas exactness column
cannot rank today's defects. The classification
records 1,807 Enron files with external-reference formulas and 736 with iterative calculation.
[Engine publication](../../crates/calc-engine/src/recalc.rs) carries `external_cache` through
reads and calculation provenance. This qualification must survive the application display and
save; it is not a newly calculated answer. Atlas also counts 577 pivot-table-bearing packages,
18 packages with model parts and 171 with query-table parts; presence alone proves no refresh
support. I found no end-to-end evidence that arbitrary imported connection/query workloads
refresh successfully.

Regional formats affect both answers and what the user reads or enters: decimal/group marks,
date separators and localized date/currency text need the actual locale context. The
[format oracle](../../apps/dna-onecalc/src/format_oracle.rs) and dc-0f1k.8 record why changing
Excel's separators and adding an LCID tag does not emulate a fully localized Excel installation;
61 answered rows need real regional-setting observations. TEXT's remaining context work is
tracked in dc-ymjf.2.2/.15/.20, alongside dc-zsn.4's locale-name work. The later notes in .15
and .20 reduce their remaining cohorts to nine comma-text and nine positive-date rows;
their original descriptions are not current counts. Do not turn missing locale into a guessed
US interpretation or claim that every tagged date name is still unimplemented.

**Acceptance observable.** Start with the committed
[catalogue oracle](../../crates/calc-func/tests/fixtures/catalogue/oracle.xlsx),
[dc_ylr linked-reference oracle](../../crates/calc-host/tests/fixtures/dc_ylr),
[circular-reference.xlsx and its recorded scenarios](../../crates/calc-host/tests/fixtures/README-circular-reference.md),
and [pivot_cube's oracle, refresh workbooks and stages.tsv](../../crates/calc-host/tests/fixtures/pivot_cube/README.md).
Use the existing classifier to extend this set with real workbook shapes, preserving their
source hashes and live Excel observations through the existing corpus route.
For each workbook, report all formula cells, matches, mismatches, refusals and missing/failed
observations, plus external/held qualifications. Change inputs, resolve an actual linked
source, exercise iteration and refresh supported pivots; compare the resulting dependent cells
and side effects. Keep disabled-code and trusted-code runs separate. Measure common-function
fixes on the whole bank without silently forgiving last-bit differences. For completion of
each named calculated workload, require **100% live matches over its full formula population**,
bit-exact numbers and exact text/errors, zero mismatches/refusals/missing answers, and matching
refresh effects. Held/external caches are separate qualifications, never included as successes.
This is a target for the workloads, not a claim the current catalogue or CUBE refresh passes.
Replay the existing [format cases](../../apps/dna-onecalc/tests/fixtures/formats/cases.tsv)
through displayed cells and TEXT under each recorded regional profile: all strings must match
exactly, and the 61 unsettled rows need observations under real regional settings, not exclusion.
Retain separate bank totals and every lost match; do not infer whole-bank completion from these
workbook sets.

**Open coverage.** dc-ymjf.2 and its remaining children (including locale .2/.15/.20),
dc-0f1k.8 and dc-zsn.4 for regional formats; dc-j9k and dc-6332 numeric work;
dc-ymjf.7 for CPU-dependent observations; dc-1nbp.9/.11 and dc-hsa5.14 for recalculation;
dc-ylr, dc-ztvv.13, dc-qahd and dc-8txy for links/evidence qualification; dc-8wp's open
calculation/layout/format children, .19–.21 for CUBE/model/server work. No bead identified for
a complete arbitrary imported-query refresh promise; do not make that promise.

### 5. The visible workbook and everyday interaction still lose important surfaces

**What the user sees.** A sheet's shapes, logos, buttons and other drawing objects may be absent
or unusable even when their package parts are retained. Chart sheets can open as preserved
sheets rather than an editable chart surface. Grid/font/selection details still differ.
An edit receipt can precede the view's update, so formula bar and grid agreement needs proof.

**Evidence.** [Drawing reader](../../crates/calc-doc/src/load/chart.rs), `read_drawing`, extracts
chart frames and marks `sp`, `pic`, groups and connectors unsupported. The new
[cell image painter](../../apps/dna-calc/src/grid/image.rs) paints `CellImage` values; it is
not an imported drawing-layer implementation. Atlas's 6,851 drawing-bearing files and the
2,524 packages with media, 1,296 with control properties and 273 with ActiveX make this broader
than cosmetic polish. dc-971p.6/.16/.43.1–.43.4 retain imported-format and rendering issues. dc-1nbp.21
identifies the outstanding-flight/mirror problem. dc-pfrp already records a same-value-entry
fix with browser evidence but awaits Excel parity. dc-zvs3 is a **renderer test-harness
navigation defect with a fix**, not evidence that ordinary users currently type into the
wrong cell.

**Acceptance observable.** Start with [dc_6yw/charts.xlsx and conditional-formats.xlsx](../../crates/calc-host/tests/fixtures/dc_6yw),
[UI30 charts-ui30-excel.xlsx and pivots-ui30-excel.xlsx](../../crates/calc-host/tests/fixtures/ui30),
and the generated `geometry`, `text`, `borders`, `panes` and `everyday` workbooks in
[the existing smoke fixture helper](../../apps/dna-calc/tests/smoke/fixtures.rs).
Extend that helper and dc-zjsg's drawing set for pictures, shapes, groups and inert controls;
these additions need Excel observations before they can pass. Compare the same workbooks
rendered in Windows Excel: anchors, crops, groups, text, charts, merged/wrapped cells,
freeze panes and selections at representative zooms. Verify keyboard edit, paste, undo, filter
and sort through the page, including delayed publication: grid and formula bar must settle to
the same committed change. Preserve unsupported objects on save and identify them visibly.
Showing an inert control is a distinct milestone from running its assigned macro/ActiveX code.
For this set require **zero missing objects or text**, exact anchor/crop/group/style and saved
relationship comparisons, and 100% of the scripted edit/paste/undo/filter/sort actions with
matching committed grid/formula-bar values. For visual completion, object bounds, clipping and
text baselines must match the Excel observations at each recorded zoom/DPI; screenshot
differences remain reported, with no new pixel tolerance declared by this note. Until those
comparisons pass, package preservation and browser smoke success remain narrower claims.

**Open coverage.** dc-zjsg.1–.10; dc-971p.6/.16/.37/.38/.43.1–.43.4;
dc-1nbp.21; dc-pfrp and dc-zvs3 for their remaining evidence. dc-zjsg.8 explicitly covers
inert controls; **no bead identified** for full interactive ActiveX control hosting.

### 6. Print/PDF output is implemented, but not general Excel page fidelity

**What the user sees.** Page Setup, preview and Print / Save PDF exist. Some workbooks refuse
printing; others can paginate differently with fonts or printer defaults. This blocks a user
whose deliverable is a formatted paper/PDF report.

**Evidence.** [Pagination model](../../apps/dna-calc-model/src/printing.rs) caps preview at
200 pages and 100,000 cells, refuses partial-area breaks and unsupported header pictures,
and uses measured Aptos Narrow 11 rates because its model lacks the Normal font. Its comments
record different measured rates for Calibri/Arial/Times. [Worker preview](../../apps/dna-calc-worker/src/printing.rs)
refuses pending calculation, unread/cached charts, comment printing and draft mode; absent
paper/orientation defaults become Letter/portrait notices. [Page printing](../../apps/dna-calc/src/printing.rs)
copies canvas content to images. dc-971p.57 records browser cases and later Windows Excel
Print-to-PDF observations, while leaving boundary/fit/font parity open. Its review comment
also questions whether coarse displayed-value provenance admits qualified calculations.

**Acceptance observable.** Use every named UI27/UI30 case in
[ui27_cases.rs](../../apps/dna-calc-model/src/printing/ui27_cases.rs), including `UNMATCHED`
and `q10-drawings-group-printed`, plus the UI30 chart/pivot workbooks named in rank 5.
[ui27.rs](../../apps/dna-calc-model/src/printing/ui27.rs) currently skips that grouped drawing
in its page-boundary test and explicitly asserts that the title-column fit cases differ;
those are open failures, not completed print fidelity. Compare Windows Excel and DNA output
on the same recorded printer, paper, DPI and fonts: page count, row/column breaks, titles,
scaling, margins, headers and drawings. Exercise the real print dialog. Resolve the provenance
question explicitly; no missing content or
uncalculated result should be silently printed as a current calculation. Larger supported
jobs must complete without truncation, or report their unsupported size before printing.
Completion for the named set requires **100% matching page counts and row/column spans**,
exact title/header/footer text and ordering, matching scale/margins/drawing placement at the
recorded device resolution, zero missing content and zero wrongly admitted uncalculated
results. Cases over the size limit must all refuse before partial output; these refusal
controls do not count as printable workbooks. The currently unmatched and skipped cases stay
in the denominator until they match. A PDF comparison must accompany the model/page tests.

**Open coverage.** dc-971p.57, with dc-zjsg for drawing content. I found no evidence of general
printer independence or full selectable/vector PDF content; neither is claimed here.

### 7. Large workbooks need responsiveness and a user calculation interrupt

**What the user sees.** A large calculation can keep running without a product interrupt.
Opening quickly does not guarantee that edits, dependent recalculation or view updates are
quick. Native add-in code can also prevent responsive recovery.

**Evidence.** The Enron report records 19 files timing out at 120 seconds, at an old revision;
dc-n2st records a 516-formula comparison exceeding 600 seconds. Those are harness timings,
not measured present-day Windows page latency. dc-dxm retains paired Excel performance
measurements and explicitly leaves current Windows/Ofgem acceptance open. PRO10's scaling
concern is still unmeasured in dc-1nbp.10. Conversely dc-1nbp.20 is closed: its host tests
record a disjoint conditional-format edit shrinking from 3,072 delivered cells to one;
the mirror now stages deltas. Its close note leaves bounded notebook paging unproved.
dc-1nbp.28 still owns the product interrupt. **No current end-to-end Windows desktop
performance baseline was verified in this assessment.**

**Acceptance observable.** On a recorded Windows machine, use the same real workbooks and
inputs in Excel and the shipped DNA page: time cold open, first calculation, one-cell edit,
full calculation, undo, save and peak memory, and retain result counts. A user interrupt must
leave a usable workbook and honest result state; late native returns cannot overwrite newer
edits. Measure shared-range fanout directly before changing the engine. Performance changes
must retain debug/perf corpus and full-bank counts under repository rules; budgets follow
the owner's post-publish direction, not a fabricated Excel-speed claim.
Make this measurable on the three Ofgem workbooks identified in rank 1 and dc-n2st's
`e54cd64b614c9b4edddf09eaaadebb02b85c323afaad02097ef98ee64d841b2e` workbook: retain five
runs of each timed operation, median/worst latency and peak memory with the build profile.
For the known stall, a proposed first threshold is **5/5 completions within the original
600-second limit on the same Linux debug comparison route**; this does not establish Windows
page responsiveness. On Windows, require **4/4 interrupt positions** (before entry, after
effect, after return, after publication, as in dc-1nbp.28) to match the E6 observations;
each cooperative case must then accept an edit, save and reopen, with zero late publications
over newer edits. An uncooperative native call must remain labelled running. A responsiveness
improvement must lower the measured median for its named operation against the pre-change
Windows build, with no worse worst-case latency or peak memory across the five repeats and
no lost calculated matches. Excel timings remain a separate comparison, not a speed claim.

**Open coverage.** dc-dxm, dc-n2st, dc-1nbp.10/.27/.28, dc-hsa5.20.5,
dc-tey4.7 for VBA speed. No separate open bead identified for the bounded notebook-paging
remainder described in closed dc-1nbp.20.

### 8. Code-execution choices and native failure recovery are not uniform

**What the user sees.** Trust decisions can differ by how a workbook is opened. A native XLL
does not gain VBA's capability restrictions simply because the app offers “Safe”. Crashing or
unloading native code can affect the desktop process and unsaved work.

**Evidence.** dc-2nn1 identifies missing origin information from the page's file input,
mapped-drive handling and the apps' differing network-path behavior. The desktop harness
tests in `tests/xll.rs` explicitly exercise marked-file policies, but do not cover all page
entry points. [AddinHost](../../crates/calc-addin-host/src/host.rs) owns a native `XllHost`
inside the app. dc-1nbp.25 is closed with the conservative change that an asynchronously used
module is retained to process exit. Its later comment records a native Windows run at
`41b8dab7` including `98099714`: **calc-xll-host 12 passed and dna-calc-desktop --test xll
16 passed, zero failed**, including async continuation and overwritten late-return tests.
The close's “not compiled or tested on this Linux host” is not the final Windows status.
Dedicated unload-after-async fixture F-b and X2/X3 lifecycle trace equality remain unverified;
the regression run is not process isolation or proof of general safe reload.

**Acceptance observable.** Open the same marked/network/local file via command line and page;
each route retains origin and applies the selected policy consistently. Denied operations are
reported and do not score as successful computations. Native add-ins have an explicit trust
decision with no claim of capability confinement. Retain the recorded Windows regression
coverage and add F-b and X2/X3 equality; fault/late-return/reload tests must demonstrate the
agreed recovery behavior and preserve the workbook without automatic replay of effects.

**Open coverage.** dc-2nn1, dc-8ovd, dc-7jz5, dc-hgie, dc-euei, dc-a3hc;
dc-6332.15 for surrogate design, dc-snz.19/.21 for native reload. These are recorded product
defects and architecture limits, not grounds to invent an extra approval workflow.

### 9. Encrypted, older and binary formats block opening or equivalent editing round trips

**What the user sees.** A password-to-open encrypted `.xlsx`/`.xlsm` cannot open even with
the correct password; there is no package decryption path. An OLE2 `.xls` cannot open.
A readable `.xlsb` can be preserved unchanged, but an edit requiring binary rewriting cannot
be saved back in that format. Users carrying encrypted or older workbooks meet the admission
refusal before any function comparison.

**Evidence.** Password-encrypted OOXML wraps the package in an OLE/CFB container with an
`EncryptedPackage` stream ([MS-OFFCRYPTO](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-offcrypto/b60c8b35-2db2-4409-8710-59d88a793f83)).
The repository's [package entry](../../crates/calc-doc/src/lib.rs) goes directly through its
[ZIP reader](../../crates/calc-doc/src/zip/mod.rs), whose `read_end_record` returns
`NotAZipArchive` without a valid ZIP directory. There is no `EncryptionInfo`/`EncryptedPackage`
reader or password input in that route. This is distinct from worksheet protection and its
password hashes, which the document model can retain. The classifier's
`unreadable_bytes_are_profiled_not_fatal` asserts refusal for OLE/CFB; the classification's
5,461 OLE2 files are **not a measured encrypted-workbook count**. No dedicated encrypted OOXML
bead or committed password-open acceptance fixture was identified.
[Save implementation](../../crates/calc-doc/src/save/mod.rs),
`SaveFormat::Xlsb`, selects `original_binary` only when no parts need rewriting; otherwise
it returns `BinaryChangesNotModelled { owner: "dc-snz.14" }`. Thus “no XLSB save at all” is
also incorrect. The Atlas excludes unreadable containers, so it understates this user's risk.

**Acceptance observable.** When encrypted OOXML is scheduled, extend the existing document
fixtures with Excel-saved password-open `.xlsx` and `.xlsm` twins: correct password opens
with identical formulas/parts and live results; wrong or missing password changes no file;
an advertised encrypted save reopens in Excel with the same protection. These are missing
observations, not a proposed plaintext fallback or a fixture authored by this review.
For BIFF8, read the retained originals against their
provenance-preserving Excel-converted twins. For XLSB, edit VBA calls, names and linked cells,
save binary, reopen in Excel and compare tokens/identities and calculated outcomes. A converted
XML copy is a separately described output, not a passed binary round trip.

**Open coverage.** No bead identified for password-encrypted OOXML; it is separate from the
owner-deferred BIFF8 work. dc-4j0e (BIFF8), dc-snz.14; dc-bctq for template
behavior. Keep these limitations visible without moving deferred BIFF work ahead of the
owner's Windows VBA/XLL focus.

### 10. A clean Windows installation and upgrade remain unproved

**What the user sees.** A working developer executable does not establish that a new Windows
user can install it, obtain WebView2, load providers, keep settings through an upgrade, or
uninstall cleanly.

**Evidence.** Packaging sources exist under [deploy/windows](../../deploy/windows), but
dc-fk66.2 still owns installer delivery and dc-fk66.3 has no completed clean-machine run in
the tracker read here. No signed-installation evidence was inspected. dc-ysl.11 is still
in progress. **dc-hi34 is closed**: its keeper close records a passing Linux check and retries
at `8184fe1f`. dc-zzsk remains open for keeper reconciliation, but its later Windows comment
records 70/70 at `2fe35920` after supplying a missing fixture (69 initially passed, then that
case passed on retry). These are revision-specific checks, not evidence that this
documentation change proves current Windows master or a release green.

**Acceptance observable.** The existing installer work installs in a clean Windows environment,
starts both apps, opens a corpus workbook, loads the fixture XLL and VBA add-in, saves, upgrades
while retaining settings, and uninstalls with recorded results. Verify version/commit identity
and the WebView2 path. The owner signs and authorizes publication after those concrete results.

**Open coverage.** dc-fk66.1–.4 and dc-ysl.11. Packaging can proceed alongside the higher-ranked
work; a successfully installed preview must still disclose its workbook limitations.

## Reading PRO9 and PRO10 against current code

[PRO9](../architecture/review-2026-11/PRO9_PROGRAMME.md) supplies experiments and work packages,
not proof its candidates match Excel. Keep the numeric research tied to the full bank and
its add-in work tied to the separate value/boundary/lifecycle/framework observations.
[PRO10](../architecture/review-2026-11/PRO10_PROGRAMME.md) was pinned earlier than this read.
Do not restart its closed Range-lifetime, edit-time refusal, edited-repair-save, volatility,
projection-copy or level-worker work (dc-1nbp.2/.12/.13/.8/.20/.24). Current tests/code and
close records show those changes. Open dc-1nbp.14–.17 also have code and tests at `03270d31`;
their remaining Excel evidence is work, not permission to reimplement the delivered changes.
Preserve qualifications: untouched repair saves, E6/E12/E13, in-book Range evidence, notebook
paging and the remaining F-b/X2/X3 Windows native-lifecycle validation (rank 8 distinguishes
those from the later passing Windows regression run).
Neither a closed bead nor a programme's old defect paragraph is a universal status claim.

## Order of work

1. **Finish the trustworthy workbook transaction.** Resolve the registered save/undo Excel
   questions and implement their answers on the existing host route (rank 1). Keep the recent
   repairs and early refusals, and demonstrate their status messages in the page.
2. **Connect native add-ins to that workbook.** Deliver dc-1nbp.3's session bridge, then
   Caller/Range/Union and page-driven event evidence (rank 2). In the same delivery, resolve
   origin/policy inconsistencies and run the native lifecycle checks from rank 8.
3. **Integrate the separate CalcVBA session's file-workflow work.** That session owns all
   CalcVBA work and coordination under Goal 4/dc-tey4. Use its sample fixes and missing
   observations to demonstrate the DNA Calc page's UDF/event/save route (rank 3); this lane
   does not start CalcVBA writes. Do not reopen the already fixed Windows 0/135 diagnosis.
4. **Make ordinary files look and print like their deliverables.** Read/preserve the drawing
   model, paint pictures/shapes and the agreed controls, settle grid/mirror correctness, and
   complete font/printer calibration (ranks 5–6). Printing depends on the same drawing content.
5. **Keep common calculation defects moving throughout.** Use Atlas frequency plus current
   bank causes and live workbook captures, including actual regional-format context (rank 4);
   give the numerical long tail its agreed smaller share. Run the high-priority Linux runner
   (Goal 1a, dc-icq4), clean-machine installers (rank 10) and web site (Goal 3, dc-6q2k) alongside
   these steps. Versioning/streams (dc-ysl.11) precede the basic-system preview; publish each
   milestone only with the owner's yes, then improve continuously (Goal 5).
6. **Measure responsiveness before optimizing it.** Deliver interruption and reproduce the
   slow/shared-range cases; then choose measured performance fixes (rank 7). After the basic
   system is published, expand analytical/Office COM coverage and binary authoring. Schedule
   encrypted OOXML admission explicitly; no owner deferral was found for it. Leave BIFF8 in
   its owner-deferred position (rank 9). Each expansion ends in an observable working user
   operation, with Excel evidence and remaining failures stated.
