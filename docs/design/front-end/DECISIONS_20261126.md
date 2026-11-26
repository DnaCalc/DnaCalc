# Front-end decisions, 2026-11-26

The owner approved the ledger-first merge method in [MERGE_PROMPT_PROVISIONAL.md](MERGE_PROMPT_PROVISIONAL.md) and asked the deciders (coordinator, Fable, Opus) to make most of the decisions. These rulings are stage 3's starting set. They override any source that disagrees.

## The aim of this round

Tighten the UX so we can start telling the stories of DNA Calc. Avoid a mess of many vaguely overlapping ideas: the book is one coherent design, and everything not taken goes to Appendix Z with a reason.

The UI makes the first impression. Automation, engine use, cross-platform and agentic tweaking also need to be foregrounded on the website. The DNA Calc app and the website should talk to each other, with links both ways and the same stories.

## Owner rulings

1. **Merge method.** Ledger first, then the book, as in MERGE_PROMPT_PROVISIONAL.md. Neon does the extraction. Fable and Opus do the clustering, the decisions and the book.
2. **Standalone Formula Explorer.** Yes, as a teaser, early.
3. **Ticked models.** The RTD clock is right for our example ticked models. Real Excel models can also be ticked from a macro engine, through value changes, volatile inputs and Calculate calls, and that is valid too. RTD has some advantages over macros plus events, but this is not set in stone.
4. **Undo across a macro run.** Try it. It is a deep challenge and will not be trivial. Design for it; do not promise it in the first release.
5. **Notes and findings.** Store them in a customXml part now. A visible-sheet mirror is optional, maybe not always on. Do something now, and expect to improve it with experience.
6. **Notebook modes.** The notebook works exactly like Jupyter. Read-only is only a way a file is opened: it cannot be edited. There is no separate read-mode presentation. A reader-style presentation belongs to the easy forms maker view.
7. **FerroUI.** Not at this stage. The WebView2/Leptos baseline stays. dc-etcxb.3 closes as not now.
8. **Website tagline.** Drop "spreadsheet for your workbooks". The book proposes a better hook.
9. **Skin architecture is central.** Fable (thinking max) and Opus (thinking extra high) each design it independently. Then one merged architecture gets our code into the right shape. After that, the UI work should be easy and beautiful.
10. **Implementation.** After the book, make beads and follow the plan. Write the new UI code fresh against the skin interface, and throw away large chunks of the current skin code. Do not plan a micro-step migration. Making a fresh UI over a good skin interface should take hours, not days. Publish it to get feedback, then keep testing and improving.

## Decider rulings on the remaining differences (coordinator, 2026-11-26)

11. **Key tips.** The Classic view uses Excel's key-tip letters exactly. Bit-matching Excel applies to the Classic view's keyboard too.
12. **View-switching keys.** Ctrl+Alt+1…5 switch views unless Excel binds the combination. The decider checks for collisions. Command search is the fallback.
13. **Hidden names as anchors.** No, as Fable argues. Anchors live in our customXml part, keyed to cells and the user's own names. We never add hidden names to a workbook.
14. **The agent's change trail.** It is kept in the file, in the same customXml part as notes and findings. It can be turned off, and the same "improve later" rule applies.
