# P14 eng10 stopping observations

These 15 small authored inputs and their recorded results come unchanged from
the eng10 delivery at `~/codex-runs/excel-results-eng10`. Each retained condition
has one trial in a fresh owned Excel process, Excel 16.0.20527.20088, Manual,
events off and MTC off. Every input starts from verified stored values. The two
Rebuild commands are separate observations. There is no new Excel execution here.

`stopping-observations.jsonl` retains the original result objects with their
source result and session receipt hashes. `sha256.json` binds these records and
the exact input packages. The original delivery retains all receipts, raw results,
source scripts, saved workbooks, omissions and failed attempts. An independent
eng14 audit verified all 6,328 delivery files, all 149 qualified trial chains,
their distinct process identities and calculation settings, and every repeated
condition's final-value agreement. There were 118 observed conditions, including
the two historical controls; two requested conditions were unrun. Most conditions
have one trial, so repeatability is not established for them.

The Boolean controls put a NOT self-reference beside a numeric self-reference
incrementing by 0.125 and a downstream `+1` reader. Values immediately below,
equal to and above MaxChange 1 distinguish logical output changes from numeric
stopping samples. A changing-error self-reference without a numeric peer ends
with #N/A on its first command at MaxChange zero and MaxIterations 2 or 5; the
second command gives #DIV/0!. Adding the numeric peer keeps it running.
The numeric-only zero-threshold and equality controls retain strict comparison.

The shared HostSession regression checks 180 final-value comparisons at unlimited
and one-evaluation budgets. Seeds remain file caches before calculation. Iterative
results still use the existing uncertified publication boundary, and no SliceV1
read certificate is claimed. Intermediate caches, general SCC execution counts,
cyclic Range targets and new observer trials remain separate questions.
