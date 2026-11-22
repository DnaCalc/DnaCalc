# Front-end design: topic area (deferred to Sunday morning, 2026-11-22)

Owner, 2026-11-20 evening: "gather everything (the designs work, brief, skin discussion) into a large topic area and defer until Sunday morning." Nothing below is to be merged or implemented until the owner has thought about how to merge it. Tracker: epic **dc-etcxb** (deferred).

## The questions for Sunday

1. **How do we merge without losing ideas?** A provisional method is in [MERGE_PROMPT_PROVISIONAL.md](MERGE_PROMPT_PROVISIONAL.md); it is not yet approved.
2. **The DNA Calc Design Book.** A complete specification of about 100 pages: every skin requirement and the skinning API. The question is how to build it from the sources below.
3. **A review of every feature** against the design. What the apps do today, what the design adds, and what changes.
4. **Planning the skin architecture.** This is the second document: the internal implementation, cleanup and refactor plan. It is [docs/plan/FRONT_END_IMPLEMENTATION_BRIEF.md](../../plan/FRONT_END_IMPLEMENTATION_BRIEF.md), which takes the design as its requirements.

5. **FerroUI as the native (and browser) front end.** A performance and fit evaluation against today's Leptos/WebView2 app: beads dc-etcxb.1-.3, report `ferroui-eval.md` in this folder once the lane fu1 and the Windows run finish.

## Since Friday (coordinator, 2026-11-22 07:15)

- **Skins internals cleaned up** (owner request, epic dc-xfrhg, closed): raw protocol intents in skin code 82 -> 0, a typed command layer over `Target`, the controller split into a core (3,054 lines, was 5,578) and a grid adapter, the desktop shell on the command path, one worker cell assembly. Map: [docs/plan/SKIN_INTERNALS_MAP.md](../../plan/SKIN_INTERNALS_MAP.md). Brief items 1 (part), 2, 4 and 10 are done; the facts section of the implementation brief was refreshed (dc-etcxb.4).
- **Classic grid parity** (dc-971p.67-.73): point mode and F4, Enter/Tab walks, Ctrl+arrow navigation, system clipboard and Paste Special, fill handle, status aggregates, context menu, sheet tabs, Find/Replace/Go To, panes, resize/AutoFit, hide/unhide, zoom, notes and hyperlinks, spill explanations, merge, CF manager, DV keyboard. The parity table in [docs/plan/UI_REVIEW.md](../../plan/UI_REVIEW.md) was rewritten to the current state. User help: docs/user/classic-grid.md.
- **FerroUI evaluation**: the Linux prototype and harness are done (dc-etcxb.2, report `ferroui-eval.md`); the Windows measurements (dc-etcxb.3) wait for Windows work to resume.
- **Site preview** (local only): 42 pages, including the Formula Explorer pages and the new docs/fun number-to-text explorer.
- Nothing from the designs has been merged.

## The sources

**Our own framing**
- [Legacy design and evidence](../LEGACY_DESIGN_REFERENCES.md) collects the pinned Strand, shell, Flow, parity and OneCalc sources for review.
- [docs/plan/FRONT_END_SCOPE.md](../../plan/FRONT_END_SCOPE.md): the one-page scope (layers, five views by persona, views in a workbook, agent customisation) and the skin status review of 2026-11-20 (section 5).
- [docs/plan/FRONT_END_IMPLEMENTATION_BRIEF.md](../../plan/FRONT_END_IMPLEMENTATION_BRIEF.md): the internal design brief, with the level-0 formula host, the Formula Explorer as skin 0, and calc-host's role still open.
- [SKIN_DISCUSSION.md](SKIN_DISCUSSION.md): the owner's direction from the 2026-11-20 UI thread, in order.
- [FerroUI evaluation](ferroui-eval.md) covers the Linux prototype, browser build and Windows measurement plan.

**The briefs** (docs/architecture/review-2026-11/pro/)
- `19-front-end-design-prompt.md`: the design brief (experience first, a cast of workbooks and people, seven storyboards, appendices A–D).
- `20-front-end-mockups-prompt.md`: full-fidelity mockups. Its changes: quiet surface, live App, Phyphox Inspector, full Notebook, a live Model map, visible gestures, and the RTD-clock ticked model.
- `21-front-end-round3-prompt.md`: the notebook as a place to compute (the Jupyter case), plus the gaps (patterns, mixed views, customisation studio, file and print, the map at scale, Guarded macros, and a fuller Formula Explorer).

**The answers**
- GPT-6 Pro, Pro 19: `docs/architecture/review-2026-11/pro/19-front-end-design-answer.md`. It includes 34 C-needs, 68 R-requirements, five push-backs and five owner questions.
- GPT-6 Pro, Pro 20: `docs/architecture/review-2026-11/pro/20-front-end-mockups.zip`, 43 interactive scenes.
- GPT-6 Pro, Pro 21: `docs/architecture/review-2026-11/pro/21-front-end-round3.zip`, 73 scenes (the 43 from Pro 20 plus 30 new): the computation notebook NC01–NC07, patterns and mixed views W01–W05, customisation U01–U05, file and print O01–O04, map at scale M03–M05, Guarded macros G01–G03, Formula Explorer E01–E03. It supersedes the Pro 20 package. To view locally, use the `pro20-mockups` server under `p21/`.
- Fable 5.2, Pro 19: [sources/pro19-fable/](sources/pro19-fable/), answer and colour-coded wireframes.
- Gemini 4 Carbon, Pro 19: [sources/pro19-carbon/](sources/pro19-carbon/), answer and 18 rendered scenes.
- Gemini 4 Neon, Pro 19: [sources/pro19-neon/](sources/pro19-neon/), answer (R-1…R-48, C-1…C-32) and 17 rendered scenes.
- Context the answers drew on: docs/plan/STRAND.md (the design language), docs/architecture/ideas/LINKED_BOXES_SKIN.md, docs/fun/names-in-motion.html and watershed.html (the model-map previews), and bead dc-hul4 (the RTD clock).

To view the mockups locally, use the `pro20-mockups` entry in C:\Work\DnaCalc\.claude\launch.json (port 8720). Its `p19/` folder holds the three non-GPT mockups with an index page.

## What the four Pro 19 answers already agree on (first reading, 2026-11-20)

- One workbook, one selection, one undo history. A change of view changes only the presentation.
- Every edit made outside the grid is labelled as a model change or a presentation change before it commits.
- The formula lens is read-only. "Rewrite as LET" is an explicit edit, shown as a preview first.
- A per-sheet view is a preference with a badge, not an automatic switch.
- The model map is a reader first. Wiring edits come later, and only on named ports.
- Boxes made on the map are cells with names on a visible sheet.
- Inspector tools are also panels in every view.
- Skins embedded in a file are declarations only. Coded components are installed, signed extensions.
- Pro's push-backs, accepted by the owner:
  - a circular reference is not a delay;
  - a dependency picture is not a recording of execution;
  - the "Excel keeps it" claim must be tested against a list of save routes;
  - keeping a file intact is not the same as being able to run it everywhere.

**Where the answers differ** (decisions for the owner):
- When the standalone Formula Explorer ships.
- Whether Run/Step on a ticked model uses F9 or the RTD clock.
- Whether undo can cross a macro run, via a snapshot.
- Whether notes and findings get an Excel-visible mirror.
- Key-tip letter parity with Excel.
- The view-switching keys (Ctrl+Alt+1…5).
- Hidden names as anchors: Fable says no.
- Whether the agent's change trail is kept in the file.
