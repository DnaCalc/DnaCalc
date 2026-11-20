# Pro 21: DNA Calc front end, round 3: the notebook as a place to compute, and the gaps

Brief for GPT-6 Pro (2026-11-20). This follows your Pro 20 mockups package, which the owner likes: "I like what it has done." Build on it. Keep its shell, Strand look, fixtures, quiet surface, scene format, review bar and verification habits. Extend the same package rather than starting again.

## Attachments

- `20-front-end-mockups.zip`: your Pro 20 package. Start from it.
- `20-front-end-mockups-prompt.md`: the Pro 20 brief, which still applies.
- `19-front-end-design-answer.md`: your accepted design.
- `STRAND.md`: the design language.
- `LINKED_BOXES_SKIN.md`: the model-map idea note.

## Why another round

We compared the package with the front-end story the owner set out. That story covers five persona views over one Excel model, a shared shell, mixed views per sheet, and agentic customisation. Its aim: "super Excel-compatible in the engine, and modern in the presentation."

Two things are needed: one large gap, and a set of smaller ones.

## A. The notebook as a place to compute (the main work)

Today's notebook scenes (F03, N01, N02) present existing sheets as a reading surface. The owner's notebook is also **Jupyter**: a place where an analyst computes, step by step, in reading order. Its outputs are live in the Pluto and Excel way: no run button, no stale cells. Design and draw that case at full fidelity, in about five to seven new scenes. Use the Sales fixture first, then Budget.

1. **Cells and outputs.** An analyst writes a formula cell and its output appears beneath it, for example `=FILTER(Orders, Orders[Status]="Pending")` with the result as a typed data frame. The next cell builds on that output by name, with `GROUPBY` or `PIVOTBY` by region and month and `SORTBY`. A third cell turns that output into an inline chart. Show the editing moment (formula bar or in-cell editor, completion, signature help) and the result appearing.
2. **Names between cells.** Each output can be named, for example `pending`, `byRegion`, and later cells refer to those names. Show how a name is given, and how a reader follows a reference back to the cell that made it.
3. **Reactive editing.** The analyst edits an upstream cell, for example changing the status filter or adding a date criterion. Every downstream output, chart and prose number updates in place. Show the moment of change: which cells recalculated, briefly marked, with nothing marked stale.
4. **A function cell.** A cell defines `=LAMBDA(...)`, for example a reusable "margin %" or "aged bucket", as a workbook name. Cells below use it. Show how a function cell looks different from a value cell.
5. **Prose and controls between cells.** Markdown-style text cells with live inline values. A small control panel bound to cells (a region dropdown, a date-range slider) drives the analysis. The live RTD price from N01 feeds one computation.
6. **Where it lives in the file, made visible.** This is the key design question; answer it concretely. Every computing cell must be a real cell or name in the workbook, so the file opened in Excel performs the same computation.
   - Propose the mapping. For example: a visible "Analysis" sheet holding the formulas in reading order (spills placed with room); names for named outputs; prose stored as presentation in the view part.
   - Show the same notebook in Classic side by side, with each notebook cell's backing range outlined, plus one line on what an Excel user sees when opening the file.
   - Cover what happens when a spill grows into the next cell's backing range, and how the layout keeps room.
7. **The live tree, as an extension.** Owner's note: "live tree as an extension". Show one formula's structure as a live, expandable tree of its LET bindings or arguments, each node with its current value, opened inline under a cell.
8. **Exploration to report.** The same Sales notebook switches from authoring to reading layout, and the analysis becomes a clean write-up. It is the same cells with a different presentation, not a copy.

## B. The other gaps we found (each one to three scenes)

1. **Workbook structure and recognised patterns.** Pro 19 section 5 and Appendix D describe pattern recognition: sheet-as-database, inputs, time series, scenarios, and pipelines. It also gives four states: "From workbook", "Suggested", "Confirmed by Maria" and "Needs review after change". No scene shows it. Draw:
   - a workbook overview of Inherited with recognised patterns;
   - the "Why suggested" and confirm/correct flow on Budget's Monthly time series;
   - Sales' Orders Table recognised as a database used from Report.
2. **Mixed views per sheet, and the file's view settings.** Show a workbook whose sheet tabs carry different views (a notebook sheet, a grid sheet and a map sheet), with each tab's view visible. Show where the file's preferred and supported views are set and what is stored, and what a reader with no DNA Calc layout sees.
3. **The customisation studio.** Pro 20 did not draw customisation. Draw:
   - "Edit this layout", with the declaration and the live preview;
   - theme tokens and the contrast checks;
   - a keymap editor showing which bindings leave the Classic conformance profile;
   - the two worked examples: an agent restyling Classic (include an agent-made dark theme variant, judged against Strand), and an agent building Loan's App from three lines, with the binding proposal and its review.
4. **The shell's file side and printing.**
   - The File screen: open, recent, templates, and new from a pattern such as "inputs, monthly model, summary".
   - Save, with a quiet note that views travel in the file.
   - Print preview for a Notebook and for a Model map region, beside the existing App print proof.
5. **Model map at scale.** Inherited at sheet-group level, with counted relationships, the refusal message for "expand all cells", and opening one sheet group into its blocks. Also an optional sheet page in the map, as the owner asked.
6. **Macros, under Guarded.** The macro security levels are now named **Off / Guarded / Unguarded**; "Safe" is renamed Guarded. Draw:
   - an App button that runs a VBA macro in Guarded mode, with the trust notice;
   - a macro that switches view and selects a block through the views library;
   - a refused action and how the refusal reads.
7. **Fixes in the existing package.**
   - M02's breadcrumb stays "Tick 0" after stepping; it should follow the clock.
   - Its chart says "Captured 1 states" after three ticks; it should count every tick.
   - Look for the same kind of slip elsewhere, and list what you fixed.

## What to deliver

- The **same package, extended**:
  - new scenes added to the existing files, or new files (`notebook-compute.html`, `customise.html`, `workbook.html`);
  - `index.html` and `verification.html` updated to cover every scene;
  - the README updated.
- Keep every existing scene working.
- Deliver as one zip.

Use the same fidelity rules as Pro 20:
- the cast fixtures, with real values that every formula's arithmetic produces;
- normal and exception variants;
- callouts beside each scene;
- no external resources and no Microsoft styling.

For new computing cells, check every shown output against the formula and the fixture. This includes `GROUPBY`/`PIVOTBY` results, which must be exact aggregates of the 25,000-row fixture. Say in `verification.html` how you checked them.

Priority if you cannot finish everything at full standard:
1. A1–A6.
2. B1, B2, B3.
3. The rest.

Finished scenes are better than many sketches. Say plainly at the end what is complete, and what you checked and could not check.
