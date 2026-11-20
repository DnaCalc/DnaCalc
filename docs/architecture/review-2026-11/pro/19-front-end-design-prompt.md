# Pro 19: DNA Calc front-end design: philosophy, views, interaction and mockups

Brief for GPT-6 Pro and the other top models (2026-11-20). Each model answers on its own; a synthesis follows. The internal implementation plan is separate (`docs/plan/FRONT_END_IMPLEMENTATION_BRIEF.md`) and takes this design as its requirements. This brief is about the design, not the code.

## The deliverable

A **design of about 20 pages**. Above all it should flesh out the **true user experience in detail**: what it is like, moment to moment, to use DNA Calc for real work. Give at least half the pages to that. The rest covers:
- a design philosophy;
- a conceptual model of the interface;
- **detailed mockups** of every view and tool;
- the interaction design.

Draw the mockups as annotated wireframes (SVG or HTML, or careful ASCII where that is clearer). Show real content, not lorem ipsum: a budget, a sales table used as a database, a loan schedule, a small systems model.

Use plain language. Mark assumptions as assumptions. End with two numbered lists:
- **R-1 ...**: the front-end requirements, each one testable;
- **C-1 ...**: the capabilities the design needs from the system underneath. State them as plain needs, for example "know which cells a formula depends on, by region", or "be told which cells changed after a recalculation". Leave out how they are built.

Order by milestones, never by calendar dates.

## What DNA Calc is

- **DNA Calc is a new spreadsheet application** that calculates exactly as Excel does. It uses Excel's files, and any workbook it saves opens and works in Excel.
- **The aim is "Excel under the hood, modern on the surface."** Someone, or an AI agent, who builds an Excel model can present it far better than a grid, and the file is still plain Excel.
- **It runs** as a desktop app and in the browser. It also runs macros and add-ins.

## The direction so far (settled; build on it)

1. **Views.** One workbook can be shown as several views, each for a kind of person:
   - **Classic:** Excel-familiar in structure and gestures, with its own visual design. Keyboard and mouse gestures matter a great deal.
   - **Inspector:** diagnostic, in the spirit of the Phyphox app. It shows what depends on what, calculation order and state, errors and where they come from, and live data.
   - **Notebook:** text and formulas in reading order, recalculating live (in the manner of Pluto and of Excel, not run-once like Jupyter). Tables and arrays act as data frames, with inline charts and controls. Layouts run down the page or across it as a pipeline.
   - **Model map:** blocks and lines, hierarchical, with optional sheet pages.
   - **App** (proposed): the model as an application for the people who use it.

   **Formula lens.** Any view can switch on a lens that shows formulas in a more readable form (names, LET and lambda structure) with the same meaning.
2. **Tools as well as views.** The first is the **Formula Explorer**: explore and test one formula, its functions, number formatting, conditional formatting and add-in functions.
   - Opened from a cell, it starts from that cell's inputs.
   - It also ships on its own, as a stripped build called "DNA Calc - Formula Explorer", with no workbook at all.
3. **One shell for every view.** Files, printing, add-ins, settings, macro security, a command palette and undo.
   - A file can name the views it prefers and supports.
   - Views can differ per sheet.
   - View settings travel inside the file in a part Excel ignores and keeps.
4. **Customisable by users and agents.** People and AI agents change how things look and behave, never what the model calculates. A skin is mostly a declaration: theme, layout, keymap, commands and panels. Code is used only for new components.
5. **No more than Excel.** A view may change only what a user could change in Excel. Everything else is presentation.

## What the design must cover

**A. Design philosophy.** What "modern on the surface" means here. How familiarity, clarity, density and honesty (never misstating the model) are balanced. What all views share in look and behaviour.

**A2. The user experience, walked through.** This is the heart of the document.
- **Write storyboards.** Write end-to-end walkthroughs for each persona, step by step, with a mockup per key moment and the exact keys, clicks and what appears on screen. Include at least these:
  - an Excel user opening a large inherited workbook for the first time, and finding their way around;
  - an analyst turning a model into a notebook write-up;
  - a reviewer tracing why one number is wrong;
  - a modeller building a small systems model on the map;
  - a manager using a finished model as an App;
  - an add-in author testing their functions in the Formula Explorer;
  - an agent and a person working on the same model together.
- **For each walkthrough, cover the details that make or break the experience:**
  - first run and empty states;
  - how features are discovered;
  - feedback while calculating or loading;
  - the transitions between views (what stays put, what moves, how the user keeps their place);
  - error, warning and refusal moments;
  - undo and recovery;
  - copy and paste between views and with other programs;
  - small interactions such as hover, focus and selection feedback;
  - how fast it feels;
  - accessibility (screen readers, keyboard only, contrast, zoom);
  - what an experienced Excel user notices as different, and why that difference is a win and not a surprise.

**B. A catalogue of real spreadsheet models.** This is the grounding for everything else.
- **Name the recurring patterns** of real spreadsheet models and of systems models. Examples:
  - sheet-as-database, used from elsewhere through lookups and filters;
  - an inputs or assumptions sheet;
  - calculation blocks;
  - time series with periods across the columns;
  - scenarios and sensitivity tables;
  - reference tables;
  - staged pipelines across sheets;
  - running balances and deliberate circular references;
  - simulations;
  - control panels built from form controls;
  - pivots;
  - live external data;
  - macro-driven models;
  - reports and dashboards;
  - stocks, flows, feedback and delays.

  Add what is missing.
- **For each pattern, say:**
  - how a person would recognise it;
  - how the program could recognise it, and how sure it can be;
  - how the user or an agent confirms or corrects that reading;
  - how each view presents it, with mockups for the important ones.

**C. The conceptual model of the interface.** Cover:
- what a user points at: cell, range, block, name, table, object, view item;
- selection and navigation that carry from one view to another;
- what an edit means in a view that is not a grid;
- undo across views;
- errors, calculation state and live data as things a user can see.

**D. Each view and the Formula Explorer in detail.** Give each:
- its mockups;
- its gestures and keyboard model;
- how a user enters and leaves it.

For Classic, write the full keyboard and mouse conformance list:
- key tips;
- navigation;
- F2, F4 and F9;
- Ctrl+Enter;
- the fill handle;
- drag-move;
- picking ranges while typing a formula;
- whatever else you would add.

Give the Formula Explorer in both of its forms, inside a workbook and standalone.

**E. Big and busy workbooks.** Real files are large and dense. The test collection holds about 16,000 real business workbooks (the Enron set), with 20 million formulas in all; some sheets are packed edge to edge. For each view, decide:
- when it shows everything;
- when it simplifies: blocks instead of cells, a run of similar formulas as one item, levels of detail, expanding on demand, sampling with a visible notice;
- when it declines, with a plain explanation and a way forward.

Say what is never acceptable: a hang, silently dropped content, or a picture that misstates the model. Give the experience budgets each view must meet: open, first paint, response to input.

**F. Customisation.** Cover:
- what a skin can declare;
- how a user or an agent makes one, tries it and shares it;
- what stays fixed, so that no skin can change a calculation.

Give two worked examples:
- an agent restyling Classic;
- an agent building an App view from a short brief.

**G. Printing and automation.** What each view prints. How macros can switch views or select things in them.

**H. Testing the design.** How we check that:
- every view opens every workbook in the collection without failing;
- the budgets hold;
- no view ever changes a value;
- the Classic gesture list behaves.

Say what screenshots a reviewer should see.

**I. Risks and open questions**, then a milestone order.

## Constraints

- No Microsoft icons, fonts, ribbon artwork or styling. The structure may be familiar, but the visual design is our own.
- Nothing may need a file Excel cannot open.
- Prefer what the program can work out from the file to anything the user must declare, and always allow correction.
