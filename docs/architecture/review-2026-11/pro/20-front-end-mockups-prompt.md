# Pro 20: DNA Calc full-fidelity mockups (follow-up to Pro 19)

Brief for GPT-6 Pro (2026-11-20). It follows your Pro 19 design, which the owner has accepted, with the changes listed below. This round makes the design **visible**: full-fidelity, self-contained HTML mockups of every view and every storyboard moment, at the standard of the attached `names-in-motion.html` and `watershed.html`. Your Pro 19 wireframes were monospace text in `<pre>` blocks. This round replaces them with screens that look like a shipped product.

## Attachments

- `19-front-end-design-prompt.md`: the original brief.
- `19-front-end-design-answer.md`: your accepted design. Its screen IDs (S01–S16, F01–F07), cast fixtures (Appendix D3), principles, conceptual model and Appendix A are the source of truth.
- `STRAND.md`: the design language. Your section 1 refinements apply (amber inner accent with a dark outer focus stroke, and the state distinctions).
- `names-in-motion.html` and `watershed.html`: the quality bar for craft, density and liveliness. Use them for that, not for their execution semantics (see your push-backs 1 and 2).

## What the owner accepted, and what changes

**Accepted as written:**
- one workbook, one selection, one undo history;
- every edit outside the grid labelled as a model change or a presentation change before it commits;
- agent proposals as reviewable units, applied together, with separate undo steps and stale-proposal conflicts;
- your five push-backs: a circular reference is not a delay; a dependency picture is not an execution recording; the lens never rewrites a formula; view settings travel in the file only through tested save routes; and a file being intact is not the same as running everywhere;
- the milestone order;
- your answers to the five owner questions. One refinement: an observed calculation order may appear as an optional Inspector detail, labelled "DNA Calc's calculation order" and never as Excel's.

**Changes for this round:**
1. **A quiet surface, with honesty kept underneath.** State shows only when it differs from normal: pending, stale, previous, error, missing source, or calculating longer than a moment. A current, clean workbook shows no status text at all, just a small steady mark. Revision numbers appear in History and in print footers, not on working screens. Keep every Pro 19 state, but surface it only when it matters. Show the same screen in its normal state and in its "something is wrong" state, so both can be judged.
2. **App inputs update results live by default.** Typing a valid value recalculates at once, and Undo reverses it. "Apply inputs" batching is an option for slow models or when the author chooses it. Show both: Loan live (default), and one model with batching switched on, with the reason shown.
3. **Inspector, in the spirit of Phyphox: live instruments.** Add a Live panel that shows:
   - RTD feeds, volatile functions and chosen cells as signals plotted over time;
   - Record, pause and clear;
   - min, max and last value, with update rate and coalescing shown honestly;
   - a recorded run you can replay against the dependency explanation.

   Design it on Stock, with the slider as the input, plus one RTD price feed added to Sales.
4. **A full Notebook.** Show:
   - a dynamic array or Table as a data frame: column types, a summary row, a row window over 25,000 orders, and sort and filter labelled "view only" versus "changes the Table";
   - inline controls bound to cells (slider, dropdown, checkbox);
   - an inline chart that updates live;
   - an RTD value in prose;
   - the horizontal pipeline layout across sheets, Inputs > Monthly > Summary, as its own screen.
5. **A Model map that feels alive.** Pick up the previews' craft:
   - hierarchy with breadcrumbs;
   - ports showing live values;
   - a source toggle;
   - a parameter rail;
   - click-to-inspect;
   - motion when values change.

   Label any motion "dependency explanation". Draw the Stock reserve loop as an iteration enclosure, and the three-day lag as a period-labelled delay. Values may animate along wires after an input changes, as long as the labelling stays truthful.
6. **Keyboard and mouse made visible.** In Classic, show:
   - the Alt key-tip overlay over the command bands;
   - range picking while typing a formula, with coloured reference outlines;
   - F4 cycling an anchor;
   - the fill handle mid-drag, with its preview;
   - a Ctrl+Enter fill;
   - drag-move;
   - the command search (Alt+Q) open with results.

   Make focus rings, selection, hover and edit states distinct, as your section 1 defines them.

## The deliverable

A set of **self-contained HTML files**:
- inline CSS and inline JavaScript only;
- no external requests, CDNs or web fonts;
- system UI and monospace font stacks, with the Strand font names first;
- no Microsoft fonts, icons, ribbon artwork or styling;
- each file opens directly in a browser.

Framing:
- Each screen is drawn at a 1440 × 900 desktop frame, scaled to fit the page.
- Where Pro 19 calls for it, give variants at 200% zoom, in high contrast, and with reduced motion.

Files, in priority order:
1. `index.html`: a gallery of every screen as a thumbnail with its ID and caption, linking into the files below.
2. `design-system.html`. Strand rendered with:
   - the tokens and type scale;
   - cell states (normal, selected, active, editing, point mode, error, spill anchor and child, stale, pending);
   - chips and cards;
   - the shell, with command bands, address and formula bar, view selector, sheet tabs and the quiet status mark;
   - drawers (model change, presentation change, agent proposal, conflict);
   - ports and wires;
   - focus, hover and selection.
3. `classic.html`: F01 plus the change-6 gestures, as clickable states, on Budget and Inherited.
4. `storyboards.html`, or one file per storyboard (2.1–2.7). Every S screen is full fidelity, in storyboard order, with step-forward and step-back controls (buttons and arrow keys) and a one-line caption per step. Apply change 1 everywhere.
5. `inspector.html`: F02 plus the Live panel (change 3).
6. `notebook.html`: F03 plus change 4, including the pipeline screen.
7. `model-map.html`: F04 plus change 5. It must be interactive: select, open a group, step back through breadcrumbs, change the reorder-point slider and watch values move.
8. `app.html`: F05 live (change 2), and the batched variant.
9. `formula-explorer.html`: F06 (docked from a cell) and F07 (standalone), plus S12 and S13.
10. `agent.html`: S14–S16 as a working review flow. You can tick units, preview, apply, undo one unit, and meet the stale conflict.

**Fidelity rules:**
- Draw real grids: column letters, row numbers, gridlines, number formats, red negatives and conditional formats, using the Appendix D3 fixtures. Use real formulas and real values, worked out correctly, never placeholder text.
- Sheets look as dense as real workbooks (Inherited really is dense). Charts are drawn in SVG from the fixture data.
- Interactions may be scripted for the demonstration, but every shown number must be what the fixture's formulas give. Use only the cast workbooks: Budget, Sales, Loan, Stock and Inherited.
- Under each screen, add short design notes: three to six numbered callouts, as in Pro 19. Keep them beside the screen, not over it.

**Quality bar:** a reviewer should be unable to tell these screens from screenshots of a carefully made, shipped product. Aim for crisp alignment and consistent spacing, with dense hands-on areas, calm paper, and colour only when it means something. Prefer finished screens to many sketches. If you cannot finish everything at this standard, deliver files 1–4 completely, then as many of 5–10 as you can finish to the same standard. Say plainly at the end which ones are complete.

**Self-check before you finish.**
- **Arithmetic and consistency:** every value matches its fixture formula, and every screen ID exists.
- **Truthful states:** each state shown says something true. The quiet surface still shows pending, stale and error states where they apply.
- **Keyboard:** storyboard steps work by keyboard.
- **Constraints:** no external resources, and no Microsoft styling.
- **Report:** list what you checked and what you could not check.
