# Pro 19: DNA Calc front-end design: the user experience, views, interaction and mockups

Brief for GPT-6 Pro and the other top models (2026-11-20). Each model answers on its own, from this text and the attachments below. We then compare the answers and merge the best of them into one design. That design becomes the requirements for a separate implementation plan. This brief is about the experience and the design, not the code.

## Attachments

- **`STRAND.md`, our design language** (v0.1): palette, type and temperament. Base your visual design on it. You may refine it; say where and why.
- **`LINKED_BOXES_SKIN.md`**: an idea note for the model map, where boxes and wires are defined names in the file.
- **`names-in-motion.html` and `watershed.html`**: two static previews of the model-map look, including feedback and delays. They are sketches to react to, not specifications.

## What DNA Calc is

- **DNA Calc is a new spreadsheet application** that calculates exactly as Excel does, oddities included. It reads and writes Excel's files, and any workbook it saves opens and works in Excel.
- **It already handles most of what real workbooks hold:** formulas and dynamic arrays, tables, names, charts, shapes, form controls, conditional formatting, macros and add-ins.
- **It runs** as a desktop app and in the browser. Keyboard and mouse come first; note where touch or pen would matter, but do not design for them now.
- **The aim is "Excel under the hood, modern on the surface."** A person, or an AI agent, who builds an Excel model can present and use it far better than in a grid, and the file stays plain Excel.
- **Six experimental views exist today:** a grid, a notebook, a ledger, an atlas, a tree and a canvas. Treat them as rough sketches, not constraints.

## The direction so far (settled; build on it)

1. **Views of one workbook, each for a kind of person:**
   - **Classic:** Excel-familiar in structure and gestures, with our own visual design. Keyboard and mouse gestures matter a great deal.
   - **Inspector:** diagnostic, in the spirit of the Phyphox app. It shows what depends on what, calculation order and state, errors and where they come from, and live data.
   - **Notebook:** text and formulas in reading order, recalculating live (like Pluto, and like Excel itself, not run-once like Jupyter). Tables and arrays act as data frames, with inline charts and controls. Layouts run down the page or across it as a pipeline.
   - **Model map:** blocks and lines, hierarchical, with optional sheet pages.
   - **App** (proposed): the model as an application for the people who use it but did not build it.

   **Formula lens.** Any view can switch on a lens that shows formulas in a more readable form, using names and LET or lambda structure, with exactly the same meaning.
2. **Tools as well as views.** The first tool is the **Formula Explorer**: explore and test one formula, its functions, number formatting, conditional formatting and add-in functions.
   - Opened from a cell, it starts from that cell's inputs.
   - It also ships on its own, as a stripped build called "DNA Calc - Formula Explorer", with no workbook at all.
3. **One shell around every view:** files, printing, add-ins, settings, macro security, a command palette and undo.
   - A file can name the views it prefers and supports.
   - Views can differ per sheet.
   - View settings travel inside the file, in a part that Excel ignores and keeps.
4. **Customisable by users and agents.** People and AI agents change how things look and behave, never what the model calculates. A skin is mostly a declaration: theme, layout, keymap, commands and panels. Code is used only for new components.
5. **No more than Excel.**
   - Every change to the model is one a user could make in Excel.
   - Layout, annotations and view settings are presentation, and are free.
   - Nothing may need a file that Excel cannot open.

## A shared cast (use these in every storyboard and mockup)

So that answers from different models can be compared, use these five workbooks:
1. **Budget.** It has three sheets:
   - Inputs (twelve assumptions, two scenario columns);
   - Monthly (twelve periods across the columns, revenue and cost blocks);
   - Summary.
2. **Sales.** A table of 25,000 order rows on one sheet, used as a database: a Report sheet reads it through SUMIFS, XLOOKUP and FILTER, and a PivotTable summarises it.
3. **Loan.** An inputs block, a 360-row amortisation schedule (PMT, IPMT, PPMT, running balance) and a chart.
4. **Stock.** A small systems model: an inventory with inflow and outflow, a reorder delay and a deliberate circular reference solved by iteration. It has a form-control slider for the reorder point.
5. **Inherited.** A real-world mess, as found in the Enron collection of about 16,000 business workbooks (20 million formulas in all):
   - 40 sheets, some hidden;
   - about 300,000 formulas, one sheet packed edge to edge;
   - links to other files that are missing;
   - a macro module;
   - names using OFFSET;
   - errors nobody has fixed.

Use these people:
- **Maria**, a finance analyst and Excel power user;
- **Sam**, a reviewer or auditor;
- **Lee**, an engineer who thinks in systems;
- **Jo**, a manager who uses models built by others;
- **Ade**, who writes Excel add-ins;
- **an AI agent** working alongside Maria.

## The deliverable

Write **one recommended design of about 20 pages**, plus appendices that do not count toward the 20.
- **Be decisive.** Where there are alternatives, choose one and give the others a line each on why not.
- **The user experience is the heart of it.** At least half of the 20 pages are the storyboards in section 2.

Use this order and these headings, so the answers can be compared:

1. **Design philosophy (1–2 pages).** What "modern on the surface" means here. How familiarity, clarity, density and honesty (never misstating the model) are weighed. What every view shares in look and behaviour. Name three to five principles that decide hard cases.
2. **The experience, walked through (10+ pages).** These are storyboards, step by step, with the exact keys and clicks, what appears on screen, and a mockup at each key moment.
   - **Write at least these:**
     1. Maria opens Inherited for the first time and finds her way around.
     2. Maria turns Budget into a notebook write-up for her director.
     3. Sam traces why one number on Sales' Report is wrong.
     4. Lee builds Stock on the model map, then checks it in Classic.
     5. Jo runs Loan as an App, tries scenarios and prints the result.
     6. Ade loads an add-in in the standalone Formula Explorer, tests a function, then uses it from a cell in Budget.
     7. The agent proposes a change to Budget. Maria reviews it, accepts part of it and undoes the rest.
   - **In each storyboard, show the moments that make or break the experience:**
     - first run and empty states;
     - how features are discovered;
     - feedback while loading or calculating;
     - the switch between views (what stays put, what moves, how the user keeps their place);
     - error, warning and refusal moments;
     - undo and recovery;
     - copy and paste between views and with other programs;
     - hover, focus and selection feedback;
     - how fast it feels;
     - accessibility (screen reader, keyboard only, contrast, zoom);
     - what an Excel user notices as different, and why that is a win rather than a surprise.
3. **The conceptual model (1–2 pages).** Cover:
   - what a user points at: cell, range, block, name, table, object, view item;
   - selection and navigation that carry across views;
   - what an edit means in a view that is not a grid;
   - undo across views;
   - errors, calculation state and live data as visible things;
   - how an agent's actions are shown, reviewed, accepted and traced.
4. **The views and the Formula Explorer (3–4 pages).** For each:
   - a full-screen mockup;
   - its gestures and keyboard model;
   - how a user enters and leaves it;
   - which of the cast workbooks it suits, and which it does not.

   Show the Formula Explorer in both forms: as a tool inside a workbook, and standalone.
5. **Real spreadsheet models (1–2 pages).** A catalogue of recurring model patterns.
   - Start from these, and add what is missing:
     - sheet-as-database;
     - inputs and assumptions;
     - calculation blocks;
     - time series with periods across;
     - scenarios and sensitivity tables;
     - reference tables;
     - pipelines across sheets;
     - running balances and deliberate circular references;
     - simulations;
     - control panels;
     - pivots;
     - live data;
     - macro-driven models;
     - reports and dashboards;
     - stocks, flows, feedback and delays.
   - For each, say:
     - how a person recognises it;
     - how the program could recognise it, and how sure it can be;
     - how the user or agent confirms or corrects that reading;
     - how each view presents it.

   The full table may go to an appendix.
6. **Big and busy workbooks (1 page).** Use Inherited. For each view, say when it shows everything, when it simplifies, and when it declines with a plain explanation and a way forward.
   - Ways to simplify include:
     - blocks instead of cells;
     - a run of similar formulas shown as one item;
     - levels of detail;
     - expanding on demand;
     - sampling with a visible notice.
   - Give the budgets the experience must meet: opening, first paint, response to a keystroke.
   - Never acceptable: a hang, silently dropped content, or a picture that misstates the model.
7. **Customisation (1 page).** Cover:
   - what a skin can declare;
   - how a person or an agent makes one, tries it and shares it;
   - what stays fixed.

   Two short worked examples: an agent restyles Classic, and an agent builds an App view of Loan from a three-line brief.
8. **Printing and macros (half a page).** What each view prints. What a macro can do to views (switch, select, show a panel).
9. **Testing the experience (half a page).** Turn the storyboards into usability-test scripts with measures of success. Say which screenshots of the cast workbooks a reviewer should see for every view.
10. **Where I would push back (up to 1 page).** Any part of the settled direction you think is wrong or risky, why, and what you would do instead. Keep it here. Do not quietly design around the direction elsewhere.
11. **Risks, open questions for the owner (at most five, each with your recommendation), and a milestone order.** No calendar dates.

**Appendices:**
- **A:** the full Classic keyboard and mouse conformance list. It should include:
  - key tips;
  - navigation;
  - F2, F4 and F9;
  - Ctrl+Enter;
  - the fill handle and double-click fill;
  - drag-move and drag-copy;
  - picking ranges while typing a formula;
  - and the rest.

  For each item, give the expected behaviour.
- **B:** R-1 ..., the front-end requirements, each testable.
- **C:** C-1 ..., what the design needs from the system underneath, stated as plain needs. For example, "be told which cells changed after a recalculation", or "know which cells a formula depends on, summarised by block". Do not say how they are built.
- **D:** the pattern catalogue table, if it does not fit in section 5.

## Mockups

- Draw them as monospace wireframes, at most 100 columns wide, with numbered callouts explained underneath.
- If you can attach files, add the same mockups as one HTML file.
- Use the cast workbooks' real content: real labels, numbers, formulas and errors, never placeholder text.
- Show the state that matters (mid-edit, mid-calculation, an error showing), not only the resting screen.

## Avoid

- Feature lists without flows.
- Generic UX advice that would fit any app.
- Mockups too abstract to build.
- A copy of Excel's look.
- Inventing calculation features Excel lacks.
- So many modes that a user gets lost.
- Hiding the grid from people who want it.

## Constraints

- No Microsoft icons, fonts, ribbon artwork or styling. The structure may be familiar; the visual design is our own.
- Prefer what the program can work out from the file over anything the user must declare, and always allow correction.

A good answer is one a designer could build a clickable prototype from, and one whose storyboards could be run as usability tests straight away.
