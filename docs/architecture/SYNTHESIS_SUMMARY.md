# Architecture review: final summary for the owner

2026-10-31. Full text: `SYNTHESIS.md` (final), detail in `SYNTHESIS_APPENDIX.md`. Four drafts, two review rounds,
then GPT-6 Pro (red-team, VBA hosting, symbolic search). Your rulings 4.7 and 4.8 hold throughout.

## What the red-team changed

- **Recalculation contract rewritten.** Execution order, what a calculation may see, and what is published are now
  separate. Each command, calculation pass and function call has its own identity, so a retry never repeats a
  side effect and a late async result for a moved or overwritten cell is rejected. "Settled" is replaced by
  freshness scoped to a command: a Manual calculation of one cell does not claim the whole workbook is current.
- **Cached VBA reads.** Excel let a UDF see a cell's old value until it was explicitly calculated (our own
  observation). Round 2 had planned to calculate first, which was wrong. Showing visible cells first can change
  what such a read sees, so we start with conservative fences and decide the policy after probes (decision 13).
- **Formula blocks** carry their dialect (`Formula` vs `Formula2`, already tracked in our code), an explicit edit
  algebra (copy, move, insert, sheet operations) and a save mode, decided before the codec work hardens.
- **Names**: showing a name's value is an explicit evaluation; caching requires the name and everything it uses to
  be pure; bindings record how they were resolved so shadowing edits rebind exactly.
- **Spills, iteration and data tables** get their own boundaries: proof that a read blank cannot be filled, an
  iteration session, and scenario transactions that never touch the base workbook.
- **VBA hosting** (four tightenings): two call contracts (procedures vs objects), early binding is not the same as
  cheap calls, common semantics with per-platform ABIs, and leases over both code and module state with an
  invocation frame for callbacks and reentry. DNA uses its own COM GUIDs with an Excel compatibility mapping.
- **Saving** pins one model revision and one result view; xlsb is saved as xlsx today (decision 12).

## Order of work

1. Now, in parallel: Excel probes that shape the APIs; tooling; the small contract types; a thin command envelope;
   clock fix and explicit contexts; the function catalogue, incrementally; the neutral VBA API with a test host and
   one real typed callback; formula-block semantics on paper and in probes.
2. The smallest end-to-end slice: one tiny workbook, serial, through the real worker, proving yield, early visible
   values, edit during calculation, a typed VBA callback that resumes without repeating, Manual scope, the spill
   blank case, save, and rejection of stale async results.
3. Then widen: spills, iteration, data tables, names, blocks, full command model, tiles, VBA object model, numeric
   research. Thread pool, asupersync and renderer choices come after the contract works.

## Decisions for you (red-team position in brackets)

1. One VBA API dependency plus one runtime link per app, once ABI details are specified. Yes. [agree]
2. Retire "one atomic recalculation result", after the contract types exist. Yes. [agree, resequenced]
3. Formula blocks now, with dialect, edit algebra and save mode first. Yes. [agree]
4. Names as nodes; explicit evaluation in the notebook; CF/spills/tables not node kinds. Yes. [mixed, adopted]
5. Exact: hardware x87 natively with CPU vendor recorded; browser refuses unmodelled transcendentals when actually
   executed; Fast stays the browser default. Yes. [agree]
6. Program search in `tools/calc-func-lab` with an xtask command; per-function claim records. Yes. [agree]
7. asupersync optional, native, tried off the critical path. Yes. [agree]
8. Run the contract-changing Excel probes first. Yes. [agree]
9. Own COM GUIDs plus compatibility mapping; reuse an Excel IID only with its full contract. Yes. [qualified]
10. Pause new UI slices and new spreadsheet.rs members until the command envelope and typed callback land. Yes.
11. Function facts once in `calc-func-catalog`, migrated in slices. Yes. [agree, incremental]
12. XLSB: declare conversion to xlsx now; same-format writing later. [new]
13. Cached VBA reads: fences now, policy after probes. [new]

Also proposed for approval as a block: the red-team's fourteen "missing decisions" (state ownership, transactions,
command retry, effect records, recovery, file preservation, security profiles, observability, memory bounds,
progress reporting and others), listed in SYNTHESIS.md section 6.
