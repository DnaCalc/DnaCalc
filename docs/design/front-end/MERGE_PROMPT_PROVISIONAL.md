# Provisional merge prompt: from the design sources to the DNA Calc Design Book

**Status:** a provisional draft for the owner's Sunday review. Do not run it. It proposes a method and the prompts for each stage. The owner decides the method, the models and the order.

## The problem

There are four independent designs (Pro 19 from GPT-6 Pro, Fable, Carbon and Neon), two or three rounds of GPT mockups (Pro 20 and Pro 21), our scope and briefs, and the owner's direction. Together they are well over 100,000 words, with many overlapping and some conflicting ideas. A single "merge these" pass will keep the loudest ideas and quietly drop the rest. The merge has to make losing an idea a visible act rather than an accident.

## The method: ledger first, book second

### Stage 1: idea extraction, one source at a time

A large-context model reads one source and writes **idea cards**, one per atomic idea:

```
id        P19-NEON-0412
source    pro19-neon/answer.md §4 Notebook ¶3 (+ mockup M-14)
kind      principle | interaction | screen | state | rule | requirement | system-need | pattern | risk | decision | push-back
topic     (from the Design Book outline below)
statement one or two sentences, in the source's meaning
why       the source's reason, if it gives one
evidence  storyboard or scene ids, fixture values
```

Coverage check: every paragraph, table row, callout and scene of the source maps to at least one card. A second model samples paragraphs and checks for any that are uncovered. Uncovered text goes back to the extractor.

These are cheap, mechanical passes. Neon (1M context) is suggested, one source at a time.

### Stage 2: clustering and dedupe, without deleting

Cards are grouped by topic. Cards that say the same thing are linked as `same-as`, so every source is kept. Cards that disagree are linked as `conflicts-with`. Nothing is removed; the ledger only gains links.

The output is `ideas.jsonl` (all cards) and `clusters.md` (one entry per cluster: its cards, its sources, agreements and conflicts).

### Stage 3: decisions

Each conflict becomes one decision row: the options, which sources support each, a recommendation and the reason. The owner rules on the decisions, and the rulings already given (README, SKIN_DISCUSSION.md) are recorded first.

The output is `decisions.md`.

### Stage 4: write the Design Book from the ledger

Each section of the book is written from its clusters and cites the card ids it uses.

The final coverage check: every card is either cited in the book, or listed in **Appendix Z: ideas not taken**, with the reason (superseded by decision D-n, out of scope, or parked for later). An idea can be rejected, but never lost.

### Stage 5: two follow-on documents (later, separately)

- **The feature review.** An inventory of what DNA Calc and the Formula Explorer do today (from the code, the handbook and the status review), mapped against the book: kept, changed, new, or removed.
- **The skin architecture plan.** The implementation brief, built out with the book's R-list and C-list as requirements. It maps each C-need onto the layers (level 0 / level 1 hosts, the view contract, the shell) and orders the work as beads under milestones.

## A draft outline for the Design Book (about 100 pages)

| Part | Contents | ~pages |
|---|---|---|
| I. Principles | aim, design philosophy, the deciding principles, honesty about state, "no more than Excel" | 6 |
| II. Conceptual model | targets, selection, model vs presentation edits, one undo, states, agents as participants | 8 |
| III. Shell | files and templates, save, print, settings, macro security (Off/Guarded/Unguarded), add-ins, command search, history | 8 |
| IV. Classic | grid, formula bar, command bands, key tips, the full keyboard and mouse conformance list (as an appendix) | 10 |
| V. Inspector | evidence, traces, calculation state, live instruments, panels in every view | 8 |
| VI. Notebook | reading and computing (the Jupyter case), cells to Excel mapping, data frames, controls, pipeline layouts, live tree | 10 |
| VII. Model map | levels, blocks, ports, delays vs iteration, the ticked clock, wiring rules, scale | 8 |
| VIII. App | authored inputs and outputs, live vs batched, scenarios, print | 5 |
| IX. Formula Explorer | in a workbook, standalone build (skin 0), function browser, formatting, add-ins | 6 |
| X. Patterns | the catalogue of spreadsheet and systems-model patterns, recognition and confirmation | 6 |
| XI. Big workbooks | budgets, simplify and decline rules per view, accessibility at scale | 4 |
| XII. Customisation and the skin API | manifest schema, tokens, keymaps, components, the extension boundary (no code in files), agent authoring, studio | 10 |
| XIII. Persistence | view part, anchors, round trips with Excel, mixed views per sheet, preferred and supported views | 4 |
| XIV. Automation | the views library for VBA, macros under Guarded | 3 |
| XV. Testing the experience | usability scripts, screenshot sets, corpus runs across views | 4 |
| Appendices | R-list, C-list, gesture list, fixtures, decisions log, source index, **Z: ideas not taken** | — |

## Draft prompt for stage 1 (per source)

> You are extracting ideas from one design source for DNA Calc's front end. Read `<source>` in full, including its mockup scenes. Write `cards-<source>.jsonl`: one JSON object per atomic idea, using the fields in MERGE_PROMPT_PROVISIONAL.md, stage 1. Use the topic names from the Design Book outline.
>
> Do not judge, merge or improve the ideas. Record what the source says, in its own meaning, with its reasons.
>
> Then write `coverage-<source>.tsv`: every section, paragraph, table, callout and scene id of the source, with the card ids that cover it. Nothing may be left uncovered. Text that holds no idea (connective prose) is marked `none` with a reason.

## Open points for Sunday

- Which models do each stage? Extraction is cheap and mechanical (Neon). Clustering and decisions need judgement (Fable or Opus, or GPT-6 Pro on the whole ledger). The book needs a strong writer with a 1M-token window (Fable, or GPT-6 Pro in parts).
- Does Pro 21 come in before stage 1? Yes, if it has arrived.
- Is the book Markdown in the repo, a Claude Doc, or both?
- How do the mockups enter the book: as embedded screenshots, as links to the HTML scenes, or both?
- Does a second GPT-6 Pro pass review the finished book against the ledger?
