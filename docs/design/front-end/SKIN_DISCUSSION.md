# The skin and front-end discussion, 2026-11-20 (owner's direction, in order)

The owner's direction from the 2026-11-20 UI thread, in order. Wording is condensed. Where a point was later refined, the refinement is listed separately below.

1. **Three paths:**
   - Review the skin status: the skin API and its abstraction, the Skin IR, whether every front end goes through it, and OneCalc.
   - The front end is a customisation interface for agents. Users and agents change display and behaviour without affecting the engine.
   - Rethink the front-end modes with personas:
     - diagnostic, in the style of Phyphox;
     - Excel compatibility (ribbon and grid, within copyright limits);
     - notebook: Jupyter, plus the live recalculation of Pluto and Excel, dynamic arrays and tables as data frames, inline charts and shapes, RTD, control panels, horizontal and multi-sheet pipelines, and a live tree as an extension;
     - blocks and lines, hierarchical, with optional sheet pages;
     - one more mode to find.

   All views sit under one shared shell for files and printing. They are skins on one engine, including thin formula rewriting, and views of an Excel model, with any extra skin detail stored in CustomXML. A file names the views it prefers and supports; views can be mixed. Every file must still open in Excel. A DNA Calc skin COM library for VBA is possible. "Super Excel-compatible in the engine, and modern in the presentation."
2. "Excel-familiar skin with our own visual design, agreed. Strong focus on keyboard and mouse gestures too."
3. Large workbooks and busy sheets: in the other views, simplify or decline them. Test the views against the corpus's large workbooks. Ground the design in concrete ideas about spreadsheet models and systems modelling, for example sheet-as-database. Write a two-page brief for GPT-6 Pro and the other top models, plus a one-page internal implementation brief.
4. OneCalc's main benefits are honest layering and a front end people can use. It exposes and tests functions, formatting, conditional formatting and add-ins.
   - Build the layering under the host and skin layer: some host and skin elements go only to CalcFml, never to the engine. OneCalc becomes the 0th skin.
   - It is not on a par with the other skins, but works the same way. The DNA Calc app has a "Formula Explorer" tool.
   - The role of calc-host in keeping this layer clean is still open: the layer may live inside calc-host, or the higher layer may avoid calc-host.
   - "DNA OneCalc" becomes "DNA Calc - Formula Explorer", a stripped build of DNA Calc with access to skin 0 only. OneCalc stays the casual internal name; a cleanup pass through the beads and docs follows (dc-sdzi1).
5. The design brief must not go into implementation detail ("inside baseball"). It asks for detailed mockups and a design philosophy. The true user experience is to be fleshed out in detail.
6. The Pro 20 round produces full-fidelity mockups like the HTML previews, covering the other concerns too. A circular reference is not a delay; we had suggested an RTD source as the tick generator for simulation models (dc-hul4).
7. "I like what it has done." The Jupyter case is not made yet. Pro 21 fleshes out the notebook as a place to compute and the other gaps, including the fuller standalone Formula Explorer.
8. Merging is deferred to Sunday morning (this topic area). The open questions: how to merge without losing ideas, the 100-page Design Book, a review of every feature, and planning the skin architecture.
9. Correction (owner): "One attribute of Jupyter is that you see the input cell and the result cell, etc. as a chain. Here it's just a presentation layout." Our briefs had mischaracterised the notebook as a reading layout ("text and formulas in reading order", Pro 19 brief; Pro 20 change 4). The notebook is first a visible chain: input formula, result beneath, next input using it. Presentation layouts are secondary views of that chain. Pro 21 section A now opens with this correction.
10. On Pro 21's computation notebook (owner): "Very good notebook progress. Maybe author layout and mode needs to be simpler, and difference between author and read not so clear. But the key ideas are expressed well." For the merge: keep the chain, named outputs and backing cells; simplify the Author layout and mode; and either make Author and Read clearly different or reconsider whether they need to be separate modes.

Related standing rules (memory): the engine does no more than Excel; views use only what Excel exposes; the desktop is WebView2 now, with a grid fast enough for WebGPU; macro security levels are Off / Guarded / Unguarded; milestones, not calendars.
