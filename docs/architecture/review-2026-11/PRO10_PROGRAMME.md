# Programme: using the pro-10 GPT-6 Pro answers

Five architecture questions were asked of GPT-6 Pro on 2026-11-12, pinned to DnaCalc `3b5aa86b` and CalcVBA
`8115aa78`. Prompts and answers are kept next to this note in `pro/`:

| # | Topic | Prompt | Answer | Careful read |
|---|---|---|---|---|
| 14 | COM-style object model (Application, Workbooks, Worksheets, Range, Names, events) and a JavaScript projection over the same core | `pro/14-object-model-and-js-prompt.md` | `pro/14-object-model-and-js-answer.md` | `pro/harvest/14-harvest.md` |
| 15 | Dependency graph and recalculation | `pro/15-dependency-graph-and-recalc-prompt.md` | `pro/15-dependency-graph-and-recalc-answer.md` | `pro/harvest/15-harvest.md` |
| 16 | Engine, host and document interfaces | `pro/16-engine-host-doc-interface-prompt.md` | `pro/16-engine-host-doc-interface-answer.md` | `pro/harvest/16-harvest.md` |
| 17 | Host-to-skin projection route | `pro/17-host-to-skin-ir-prompt.md` | `pro/17-host-to-skin-ir-answer.md` | `pro/harvest/17-harvest.md` |
| 18 | Concurrency and isolation | `pro/18-concurrency-and-isolation-prompt.md` | `pro/18-concurrency-and-isolation-answer.md` | `pro/harvest/18-harvest.md` |

Tracker: epic **dc-1nbp**. The careful reads (dc-1nbp.1, closed) give every answer section a disposition, check
the answer's claims against the code at the pins, size the experiments in Excel runs, and propose the beads below.
Brief every lane with its harvest file as well as the answer.

## What the reads found

The answers saw excerpts, so much of what they rank as risk is already handled in code, with tests; each harvest
lists those. Their value is in the defects they led the readers to, and in the experiment designs. The defects:

- **14**:
  - A VBA UDF fails for the rest of the project's life after 4,096 distinct Range objects, because the table is
    never released (`calc-vba/src/runtime/spreadsheet.rs:360-387`).
  - DNA Calc desktop gives every add-in a workbook named Book1 with no active sheet, whatever file is open. Add-ins
    never receive SheetChange or AfterCalculate.
- **15**:
  - Excel's measured volatility rule (F2) is not applied: once a formula has a volatile function, it stays volatile,
    at four sites in `recalc.rs`.
  - Defined-name bodies count toward the caller's volatile flag.
  - SHEET, SHEETS and FORMULATEXT are not marked volatile.
  - When many formulas read a range of formula cells, the dirty set grows as producers × readers (an unmeasured
    scaling risk; a possible cause of the slow Enron books).
- **16**:
  - Edits that the save will refuse are accepted, and only Undo gets out of it.
  - Excel's repairs on load (such as a duplicate `sheetId`) are never written on save, so Excel refuses the saved
    file.
  - CalculateBeforeSave is always false.
  - Undo of a structural edit drops every displayed value.
- **17**:
  - Any conditional-format rule makes every edit reset every subscription, so each edit re-sends the whole visible
    area.
  - The page mirror is copied whole for each publication.
  - The guarantee that a reply finds its cells applied fails while a flight is outstanding.
- **18**:
  - The level-parallel route, which is only used in tests today, wrongly refuses formulas with `$` add-in calls.
  - An async completion can let a library unload while the add-in's own thread is still running its code.
  - There is no user calculation interrupt.

Where an answer conflicts with an owner rule, the harvest says so and the rule wins:
- Bit-match Excel, so we follow Office.js as measured, not a `DnaExcel` dialect.
- The WebGPU-friendly grid comes from the start.
- Performance budgets come after publish, and are never checks on every commit.
- No process-only stages.

## Beads

| Bead | From | Work | Priority | Start first |
|---|---|---|---|---|
| dc-1nbp.2 | 14.a | VBA Range objects: release them; the limit counts live objects only | 1 | yes |
| dc-1nbp.3 | 14.b | DNA Calc desktop: native add-ins see the real workbook and its events | 2 | yes |
| dc-1nbp.4 | 14.c | COM: the same answer on every route for advertised members | 3 | |
| dc-1nbp.5 | 14.d | One persistent Range identity for UDF, event and COM routes (design note; after .6) | 2 | |
| dc-1nbp.6 | 14.e | Excel object-model probe batch (about 156 desktop runs) | 2 | yes |
| dc-1nbp.7 | 14.f | VBA event code: Worksheet and Workbook objects over the session loan | 2 | |
| dc-1nbp.8 | 15.a | Excel's volatility rule at publication; name bodies out of the flag; three missing flags | 1 | yes |
| dc-1nbp.9 | 15.b | Run-time volatility events (after .11) | 2 | |
| dc-1nbp.10 | 15.c | Shared ranges of formula cells: measure the closure, then bound it | 2 | measurement half |
| dc-1nbp.11 | 15.d | One Excel batch for answer 15 (about 120 desktop runs) | 2 | yes |
| dc-1nbp.12 | 16.a | Refuse at edit time what the save would refuse | 1 | yes |
| dc-1nbp.13 | 16.b | Write Excel's repairs on save, and show them | 2 | yes |
| dc-1nbp.14 | 16.c | CalculateBeforeSave from the workbook (after .19) | 2 | |
| dc-1nbp.15 | 16.d | Keep displayed results through Undo restore and table rebuild (after .19) | 2 | |
| dc-1nbp.16 | 16.e | Late-return regression cases for structural edits | 3 | |
| dc-1nbp.17 | 16.f | Sheet rename and remove with tables, charts and rules (after .19) | 2 | |
| dc-1nbp.18 | 16.g | Array caches under Manual Save(false) (after .19) | 3 | |
| dc-1nbp.19 | 16.h | Excel experiments from answer 16 (E7, E12, E13 first: about 40 runs) | 2 | yes |
| dc-1nbp.20 | 17.a | Projection amplification: conditional-format resets, staging copy, recounts, notebook paging | 2 | yes |
| dc-1nbp.21 | 17.b | The reply-visibility guarantee and the two mirrors a view reads (Opus decision, then Sonnet) | 2 | yes |
| dc-1nbp.22 | 17.c | Excel round trip of the DNA view manifest and locator names (11 desktop runs) | 2 | yes |
| dc-1nbp.23 | 17.d | Excel facts about names for a presenter and the boxes view | 3 | |
| dc-1nbp.24 | 18.a | Level-parallel workers must not refuse formulas with user calls | 2 | yes |
| dc-1nbp.25 | 18.b | Native async completion is not module quiescence | 2 | yes |
| dc-1nbp.26 | 18.c | Correct three design statements about threading | 3 | |
| dc-1nbp.27 | 18.d | Excel batch: calculation interrupt, macro read frontier, MTR iteration, `$` 64 under F9 | 3 | |
| dc-1nbp.28 | 18.e | User calculation interrupt on the product route (after .27) | 3 | |
| dc-1nbp.29 | | Deferred parts, revisited each usage week | 3 | |

The harvests' notes for existing beads were added as comments on:
- dc-hsa5.20.10, dc-hsa5.20.2, dc-hsa5.14, dc-hsa5.6;
- dc-hsa5.12, dc-8ovd, dc-snz.5;
- dc-0f1k.24, which now depends on dc-1nbp.3;
- dc-hsa5.20.5, dc-6332.15.

## Order

Ordered by what helps a published DNA Calc most; no calendar estimates.

1. **Defects a user hits.** None of these needs Excel first; all are Sonnet-sized with an Opus read of the close:
   - dc-1nbp.12 (refused saves);
   - dc-1nbp.2 (the 4,096 Range limit);
   - dc-1nbp.13 (repairs on save);
   - dc-1nbp.20 (whole-view resets on any conditional format).
2. **Engine correctness:** dc-1nbp.8 (volatility rule), then dc-1nbp.24 and dc-1nbp.25. Sonnet or Codex.
3. **Excel batches on the Windows desktop.** The Excel runner may run at any time:
   - dc-1nbp.22 (11 runs), dc-1nbp.19 (40 runs first), dc-1nbp.11 (120), dc-1nbp.6 (156);
   - their answers then release dc-1nbp.5, .9, .14, .15, .17 and .18.
4. **Add-in focus:** dc-1nbp.3 (real workbook and events for native add-ins), then dc-0f1k.24.
5. **The rest** as their prerequisites land; dc-1nbp.29 is reviewed each usage week.

## Rules that apply to every bead here

- Excel-true: the engine does no more than Excel; the same model runs in Excel.
- Experiments freeze their predictions before asking Excel. Desktop runs use owned Excel sessions on the private
  desktop and never change Trust Center, macro security or registry settings.
- Close notes quote numbers from tool output that the closing agent ran.
