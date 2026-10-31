# Question for GPT-6 Pro: DNA Calc's medium-term strategy and its validation at corpus scale

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet (native Windows/Linux and
browser via wasm; live at dnacalc.com), with a sibling engine-free front end (DNA OneCalc) for
formulas, formatting and UDFs, an independent VBA engine with a JIT (CalcVBA), and a continuously
built Excel Functions Handbook. Its target architecture has just been reviewed (Appendix A is the
summary). State today: about 500 functions; per-function bit-exact research against 3.9 million
recorded Excel answers (83% exact after removing fitted approximations; symbolic program search
under way); a 21-workbook test corpus where 93.6% of 20,030 formula cells match Excel's cached
values (refusals count as failures); three AI coding workers (two Linux machines, one Windows
machine with Excel) plus an orchestrating agent; one owner.

The owner wants to pin down a medium-term strategy, sequenced towards milestones (no calendar timeline; pace is unknowable now):
1. **What DNA Calc should be really good at being and doing**: the few things it must do better
   than Excel or any alternative, for whom, and what it should deliberately not do. Propose two
   or three coherent positions (for example: the reference-grade Excel-compatible engine and
   function oracle; the programmable/agent-customisable spreadsheet; the fast, explainable,
   debuggable spreadsheet for large models), with what each implies for scope, architecture
   priorities and the first public milestones, and recommend one, with reasons.
2. **How to realise and validate it.** A test strategy based on very large-scale corpus
   recalculation: which public spreadsheet corpora exist (for example Enron, FUSE, EUSES, VEnron,
   GitHub-mined workbooks, Microsoft sample files), their licences and suitability, how to
   sample and stratify them, how to compare (cached values, recalculated values in live Excel on
   several builds, formatting, spills, errors, file round trip), how to triage and shrink
   failures, and how to keep the headline honest (refusals and unsupported features count as
   failures).
3. **A Windows/Office test environment that is easy to manage**: options for running many Excel
   instances for automated recalculation and probing (cloud VMs, Windows 365 / Azure Virtual
   Desktop, on-premises machines; Microsoft 365 Apps licensing such as shared computer
   activation; several Excel builds/channels and UI languages; AMD vs Intel CPUs), automation
   approach (COM from many processes, isolation, crash recovery, throughput per core), rough
   costs, and what Microsoft's terms allow for automated, unattended use.
4. **Scavenging Excel "tips and tricks"** for quirky and newer behaviour that corpora do not
   represent, as a continuous validation source (we are preparing a separate catalogue; suggest
   how to keep it current as Excel ships new functions and behaviours).
5. The validation programme as an ordered sequence of milestones, each with its exit criteria and first measurable outcomes (no calendar durations).

Be concrete, cite sources for corpora and licensing, and say where you are unsure.

---

# Appendix A: architecture synthesis summary

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
