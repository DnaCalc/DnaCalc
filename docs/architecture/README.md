# Architecture

The target architecture of DNA Calc, from the review of October 2026.

Current documents (start with the summary):
- `SYNTHESIS_SUMMARY.md`: one page for the owner, with the open decisions.
- `SYNTHESIS.md` and `SYNTHESIS_APPENDIX.md`: the agreed target architecture and its detail.
- `BRIEFING.md`: scope and the owner's principles and rulings (section 4), which override the rest.
- `RECALC_CONTRACT.md`: the recalculation contract for the first end-to-end slice.
- `EXCEL_CALC_PROBES.md`: Excel calculation experiments that decide open points of the contract.
- `XLL_COM_HOSTING.md`: hosting .xll add-ins and the COM object model.

`review-2026-10/` keeps the record of how the synthesis was reached: the foundation risk audit
(`risk-*.md`), library research, ideas recovered from the early Foundation plans, four independent
drafts (Opus 5.5, Fable 5.1, GPT-Astra 6, Gemini 4 Argon), two review rounds, and the GPT-6 Pro
questions and answers (symbolic search, VBA hosting, red-team, calculation semantics, Excel quirks,
strategy and validation, recalculation contract). Paths inside these files refer to their original
working folder, `C:\Work\DnaCalc\parked\arch-review\`. The records are verbatim
except that two words the project does not use were replaced with neutral ones (check, latch).
