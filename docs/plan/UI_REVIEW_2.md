# DNA Calc UI: second look

Reviewed locally on 2026-10-31, after the Tree, Notebook and pane/fitting work.
Grid editing and the inspection skins are much more useful. Imported workbook appearance,
phone workspace height, and recovery from array errors still prevent Excel grid parity.
The epic remains open: dc-971p.6 and the unmeasured parts of dc-971p.16 are unfinished.

## Evidence

Used each skin at **390×844, 1024×768 and 1440×1000**, device scale 1: opened
`features.xlsx`, selected wrapped B5, traced cut-base B3, filtered Ledger refusals,
revealed Tree B3, edited/linked Notebook sources and inspected an actual blocked array.
OneCalc received scratch inputs, SUM and SEQUENCE; its result and input workflow was used
at all three widths. Screenshots: `~/codex-runs/ui-shots/review2-*`; pane comparisons:
`panes-{before,after}-*`; Tree/Notebook: `{tree,notebook}-{before,after}-*`.
These are local development bundles, with no website publication. Chrome supplies the
screenshots; Chrome and Firefox exercise the existing application smoke route.
Linux check: 282 passed, zero failed/ignored. Calc browser smokes: 443 passed, zero failed in each browser. OneCalc: 50 passed, one failed in each browser.
Corpus before/after: 21 workbooks, 20,030 formula cells; 19,284 matched, 659 failed
(652 mismatched, six refused, one missing); 73 external-link and 14 circular-rule cells
listed separately; 96.27% pass over all formula cells. No new numerical agreement claim.

## What still keeps Grid from Excel parity

- **dc-971p.6, P1:** the package's H1:I1 merge still has two grid cells; B2's rich-text
  runs flatten to one style. Ordinary fills/borders and font fidelity/format authoring are incomplete. Numeric overflow and
  unwrapped text overflow still use an approximate digit-width estimate. AutoFit measuring
  the rendered text does not establish Excel pixel parity. The base workbook font maps to
  Strand's system font; other families need to exist on the browser's machine.
- **dc-971p.16:** first/scrolling origins and two/four split views now render, scroll and retain
  source identity. Positions adapt to available space. Current split scrolling is session
  state; saving preserves the original imported view, rather than authoring new split positions.
  Live Excel readings for scrolled-origin freezing and boundary deletion remain unavailable.
  Scrolled active-cell freezing and edits through an offset frozen band refuse explicitly.
  Merged fitting, wrapped-column fitting and combined wrapping/rotation remain unavailable.
- A blocked Grid formula still gives a generic #SPILL! explanation. Notebook can show the
  actual blocking area and open it; shared Grid details need that same recovery route.
- Literal-source Replace still lacks wildcard/displayed-value replacement. Broad workbook
  operations and value-dependent inventory filters retain the performance work in dc-gf0.

## Skin-by-skin judgement and information cuts

| Skin | Useful now / remaining friction | Information removed or folded in this review |
|---|---|---|
| Grid | Stronger entry, navigation, clipboard, geometry and worker-backed history. The 390 px editing area shrinks sharply when the changed-file title wraps. | Existing clipboard/axis/pane tools stay folded. No result-state or pending/failure information removed. |
| Canvas | Keyboard tracing and source recovery work. At 390 px, the control rows and inspector leave little graph height. It remains a dependency inspector. | Folded the repeated scope/limits/editing explanation into About and successful-calculation revision metadata into existing details; actual omissions and error/refusal reasons stay visible. |
| Atlas | Coverage is clearly different from agreement with Excel; refusal groups lead to real cells. Phone sheet columns still need horizontal scrolling. | Runtime/memory/revision details already fold under Calculation diagnostics. Keep counts and the agreement explanation: both prevent misreading the overview. |
| Ledger | Desktop formula/value/state comparison works. At 390 px only address/formula appear initially; value and provenance are off to the right. | Counts distinguish scope/filter/page and remain. Read counts and format metadata already fold into row Details. |
| Tree | Search, retained folds, keyboard focus and actual child values work. Physical row scaffolding remains a deliberate worksheet outline. | Hid exact duplicate literal input text at every width; folded the repeated formula-scope explanation into About. Result and provenance remain. |
| Notebook | Cell-backed creation, saved link order and shared editing work. Four or six action buttons repeated on every block still overwhelm a phone. | Folded the repeated ordering/dependency explanation; removed the repeated literal result below selected prose’s editor; the projection retains its source value. |
| OneCalc | Formula, result, scratch inputs and saved sessions have a clear reading order. The formula workbench remains distinct from a workbook. | Calculation/format/VBA/session detail is already folded. No additional always-visible information justified removal. |

## Inconsistencies and next work

Calc's saved dynamic formula is displayed as `=_xlfn.SEQUENCE(...)`; typed OneCalc shows
`=SEQUENCE(...)`. Both use the shared editor, but friendly formula spelling is not shared.
Grid exposes a generic spill error while Notebook offers an actual blocker link. Notebook's
block input is labelled, while the same selected source remains in the shell formula bar;
keep the shared edit session and avoid introducing another draft/evaluator.
The fresh OneCalc browser VBA module-replacement case returns #VALUE! instead of 52;
that belongs to existing dc-snz.5/VBA work, not a second implementation in the UI.

New dc-971p children, in priority order (all left open; none started):

| Priority | Bead | Concrete next outcome |
|---|---|---|
| P1 | dc-971p.17 | Shared Grid details show the real current array owner/bounds/blockers and navigate to the blocker using the existing worker detail projection. |
| P1 | dc-971p.18 | A changed or long workbook title does not consume an extra toolbar row; preserve file identity and all commands accessibly at 390 px. |
| P1 | dc-971p.19 | A phone row shows address, value and state together; formula/recovery details remain reachable without an undisclosed sideways search. |
| P2 | dc-971p.20 | Calc skins and OneCalc display friendly function spelling through one calc-fml-or-below display route, preserving save syntax and evaluation. |
| P2 | dc-971p.21 | Fold inactive block actions, keeping source selection, keyboard access, current results and explicit link/order commands. |

The existing dc-971p.6 stays ahead of these; dc-971p.16 keeps its measurement and unsupported
layout work. No new bead duplicates either package or the existing large-workbook work.

Full smoke timings: Calc 189 s Chrome / 157 s Firefox; OneCalc 19 s Chrome / 18 s Firefox. Repeated full suites and optimized WASM rebuilds
(about four minutes each) slowed individual slices; browser cases remain outside the per-commit
check. Windows checks, live Excel and device touch/IME were not run here.
Pane semantics follow the documented [OOXML pane attributes](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.pane?view=openxml-3.0.1),
[first-view origin](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.sheetview.topleftcell?view=openxml-3.0.1)
and [Excel split areas](https://support.microsoft.com/en-us/excel/split-panes-to-lock-rows-or-columns-in-separate-worksheet-areas).
The synthetic pane/style variants extend the existing package-part fixture editor; they are not
new Excel measurements or captured oracle answers.
