# Question for GPT-6 Pro: a catalogue of Excel quirks and newer behaviours, as executable probes

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet (Microsoft 365 behaviour,
real xlsx/xlsb files, about 500 worksheet functions, bit-identical results as the goal). We
validate against (a) recorded Excel answers for individual functions (about 3.9 million readings)
and (b) recalculating real workbooks and comparing with the values Excel cached in them. Neither
covers quirky, edge-case or recently introduced behaviour that rarely appears in real workbooks.
We want to mine "tips and tricks", known gotchas and newer features as a third validation source:
each item becomes a small probe we run in Excel (build, platform and CPU recorded) and in DNA Calc.

Please produce a catalogue of at least 300 items, grouped by area, as a Markdown table per area
with columns: id, area, behaviour in one sentence, probe (cell setup and formula(s), exact text),
expected Excel result as you understand it (or "measure"), why it is a trap for a
re-implementation, source (link to Microsoft documentation, a well-known Excel expert's site or
book, a Stack Overflow / Microsoft Q&A thread), and confidence. Areas, at least:
- Arithmetic and comparison: floating point display vs stored value, 15-digit display, the
  famous 0.1+0.2 style cases, rounding of near-ties, -0, comparison of text and numbers, empty
  vs "" vs 0, implicit conversion of text to numbers and dates, operator precedence (-2^2).
- Dates and times: 1900 leap-year bug, 1904 system, negative times, text-to-date parsing by
  locale, two-digit years, DATEVALUE/TIMEVALUE edge cases.
- Text: case-insensitive comparison and its Unicode limits, wildcards and ~ escapes in criteria,
  TEXT format codes, CHAR/CODE/UNICHAR, LEN with surrogate pairs, FIND vs SEARCH, TEXTSPLIT and
  the newer text functions, regex functions if present.
- Lookup and reference: MATCH/VLOOKUP approximate match on unsorted data, XLOOKUP modes and
  binary search, INDEX returning references, OFFSET/INDIRECT volatility, implicit intersection,
  whole-column references, 3D references, structured references in tables.
- Dynamic arrays and LAMBDA: spill rules and #SPILL!, @, Formula vs Formula2, arrays in
  functions that did not support them before, LET/LAMBDA recursion and limits, MAP/REDUCE/SCAN,
  GROUPBY/PIVOTBY, TRIMRANGE and range operators, TOCOL/WRAPROWS, newest functions.
- Aggregation and criteria: SUMIF/COUNTIFS criteria parsing ("<>", "=", wildcards, numbers as
  text, dates), SUBTOTAL/AGGREGATE with hidden and filtered rows, error handling options.
- Errors: which functions propagate vs swallow which errors, #N/A vs #VALUE! choices, error
  ordering when several arguments are errors, IFERROR/IFNA/ISERR differences.
- Statistics and finance: known inaccuracies and changed algorithms (e.g. old vs new statistical
  functions), RATE/IRR convergence and starting guesses, NPV/XNPV quirks.
- Names, tables and workbook structure: scope and shadowing, names that look like references,
  relative names, table auto-expansion, external links, circular references and iteration.
- Calculation and volatility: what recalculates when, manual mode traps, precision as displayed.
- File and locale behaviour: what changes when a workbook moves between locales, R1C1 vs A1,
  CSV round trips.
Prefer behaviours that are observable from a cell formula. Mark items that changed between Excel
versions. Do not reproduce long copyrighted text; one-sentence descriptions plus links are enough.
