# Enron workbooks: DNA Calc against the saved cached values (2026-11-11)

Root causes are filed as beads, listed in the table column "Bead".
Run on dna-koderbot (Linux x86_64), repository commit 8081154a plus tracker pulls, debug build.

## What this compares, and what it is not

The 15,871 `.xlsx` files of the Enron spreadsheets dataset (figshare 1221767, `spreadsheets.7z`,
R2 `dnacalc-corpus/sources/enron/20261111/`) were opened and recalculated by DNA Calc, and each
formula cell's result was compared with the value saved in the file. The saved value is
`UnknownFreshnessCache` evidence: it can be stale, it carries 17 significant digits, and in a
file saved in manual calculation mode it may never have been recalculated. **A saved-value
agreement is not a pass.** The live `pass` count of this run is 0 by rule (no live Excel capture);
every number below is "saved-value agreement" or "saved-value disagreement".

Provenance of the saved values: the dataset's own note says its authors converted the original
`.xls` files to `.xlsx` with Excel 15, and every package here says `Microsoft Excel` 15.x in
`docProps/app.xml` with a `fileVersion` element. No Excel-save record is retained, so the
`--saved-excel` manifest (built by a script outside the repository from each package's own
metadata, keyed by SHA-256) records exactly that basis as its evidence. Treat it as an
assertion by us, not as retained Excel evidence.

## How it was run

`cargo xtask corpus <dir> --no-capture --dna-only --saved-excel <manifest> --timeout 120
--shard i/N --resume`, existing tooling, no change to it. The first run used 6 shards and was
stopped (my own processes only) after the disk fell to 2.9 GB free: each workbook leaves about
20 MB of raw JSON (240 MB for the large ICE-volume files). From then on a janitor script (outside
the repository) kept a compact per-workbook `offenders.json` (all cells whose saved-value verdict
is neither matched nor not-run, capped at 3,000 cells per workbook) and moved the large JSON
files to `.tar.zst` archives, which are on R2. The 7,101 workbooks completed in the first run
have no shard `summary.json` (it was killed before it was written), so `--merge` could not be
used; totals below are the sum of the per-workbook `summary.json` files. The remaining files
were run as 4 more shards (all finished).

## Totals (sum of per-workbook summaries)

| | Count |
|---|---:|
| `.xlsx` files | 15,871 (15,869 distinct by hash) |
| Workbooks compared (formula cells present) | 9,201 |
| Files with no formula cells | 6,649 in the second run, not compared |
| Timed out at 120 s (second run) | 19 files (known formula counts kept as failures by the harness) |
| Formula cells in compared workbooks | 19,666,149 |
| Live pass | 0 (by rule: no live capture) |
| Saved-value agreement (matched, DNA value equals saved decimal) | 12,961,186 (65.9%) |
| Matched from another workbook's link cache | 1,921,404 (9.8%) |
| Disagree: mismatched | 33,861 (0.17%) |
| Disagree: refused (typed not-modelled) | 476,695 (2.4%) |
| No saved answer used (failed or missing: volatile-dirty readers, downstream of refusals, array caches) | 4,273,003 (21.7%) |

Reading: Everything not matched counts as failing in the
headline, as the honesty rules require. The 1,288,450 cells excluded as
`volatile-dirty-or-unclassified-dependency` and 473 array-shape caches are in the 4.27 M.
I did not break the remaining ~3 M "not run" cells down further; most read a refused cell
(`MissingInput`, 196,605 listed cells in `offenders.json`, which is capped, so more exist).

## Disagreeing cells: saved value not meaningful vs real

The 44,382 distinct mismatched cells listed in the offender files (33,861 in the mismatched count;
the rest are mismatches inside the link-cache bucket) were joined with each workbook's
`calcPr` (read-only zip parse):

| Reason the saved value may not be meaningful | Mismatched cells | Share |
|---|---:|---:|
| Workbook saved in manual calculation mode (`calcMode="manual"`) | 36,186 | 81.5% |
| Iterative calculation on (`iterate="1"`), saved values depend on iteration history | 3,926 | 8.8% |
| Reads external links (saved values come from other files' caches) | 2,072 | 4.7% |
| 1904 date system: none left after the above | 0 | 0 |
| None of these: automatic, no links, no iteration | 2,198 | 5.0% |

(The classes are applied in the order of the table; a workbook counts once, in the first that
applies.) A manual-calculation workbook is not proof of staleness: it only means Excel did not
promise to recalculate before saving. Small-ulp numeric disagreements (below) are real in any
mode. Volatile functions (NOW 4,621, TODAY 6,168 refusals in the capped lists, 1,496 and 534
workbooks) need no saved comparison: the harness already excludes them and their readers.

Largest manual-calc groups (all but the first need a look at the cached text, so the causes are
inferred, not proved): text results of lookups into data the workbook refreshed from outside
(`IF/ISNA/VLOOKUP`, 17,949 + 3,679 + 1,853 cells in 9 FTCan_Gas_Bench-family workbooks; DNA
gives `not used`/`-`, saved `CANADA...`); an add-in function (`XLL.EURO` 2,724 cells, `XLL.HPVAL`
638, `NEXTMONTH` 360: DNA has no add-in, so `#NAME?`); `#N/A` vs `#REF!` (559).

## Real root causes (groups, counts, minimal pattern)

Refusals first (capped lists; counts are lower bounds, workbooks in parentheses).

| # | Root cause | Cells | Workbooks | Minimal pattern (authored here) | Bead |
|---|---|---:|---:|---|---|
| 1 | `TEXT` not implemented | 24,341 | 161 | `=TEXT(A1,"mm/dd/yyyy")` where A1 holds a date serial | dc-f35o |
| 2 | Sheet-qualified name refused to compile (dc-10y leftover) | 6,823 | 98 | `=Sheet2!Rate*2` where `Rate` is a sheet-scoped name | dc-9cyc |
| 3 | `Compile:Reference:QualifiedNonReference` | 3,270 | 58 | a sheet-qualified token that does not resolve to a reference | dc-x8v4 |
| 4 | Argument coercion not implemented: `YEAR` arg 1 (2,234), `DATEVALUE` (1,838), `VALUE` (881), `-`, `+`, `*` arg 1 or 2 (~2,500), `MONTH` (72) | ~7,500 | ~35 | `=YEAR("2001-01-16")`, `=A1*2` where A1 holds text such as `"1,000"` | dc-3k5n |
| 5 | D-functions' database or criteria argument: `DSUM` arg 1 (228), `DMAX/DMIN/DSTDEV/DAVERAGE` arg 3 (90 each) | ~680 | 2 | `=DSUM(A1:C9,"x",E1:E2)` with a computed criteria block | dc-r526 |
| 6 | `NOW`/`TODAY` (volatile) | 10,789 | 1,496 / 534 | `=TODAY()` | dc-uar (existing) |
| 7 | `CELL` argument 2 | 565 | 199 | `=CELL("filename",A1)` | existing refusal, not filed |
| 8 | `COUNTIF`/`SUMIF` argument shapes | ~700 | ~25 | `=SUMIF(A:A,B1,C:C)` with a reference argument shape the kernel refuses | not filed (small) |
| 9 | Downstream: `MissingInput` (reads a refused cell) | 196,605 | 472 | a formula reading a cell of group 1 | not a root; fixed by 1-5 |

Mismatches that remain after removing manual-calc, iterative and link workbooks (2,198 cells),
plus the small-ulp numeric classes that are real in any mode:

| # | Root cause | Cells | Workbooks | Pattern | Bead |
|---|---|---:|---:|---|---|
| 10 | `SUM`/`-` returns a tiny nonzero (about 1e-15, bits `bce0...`) where Excel saved 0: Excel's rounding of a near-zero add/subtract result to zero | 1,297 (101 outside manual workbooks) | 31 | `=SUM(A1:A3)` over 0.1, 0.2, -0.3 | dc-ss3b |
| 11 | `CORREL` (3,361 cells, 1-4 ulp), `NORMDIST` (180), `FINV` (8) off by a few ulp | ~3,550 | ~25 | `=CORREL(A1:A20,B1:B20)` | dc-j9k (existing) |
| 12 | Bond functions: `YIELD`, `MDURATION`, `IRR` around the scott_neal Munis model, with number differences above 64 ulp and `#VALUE!` instead of a number | ~650 | ~15 | `=MDURATION(settle,maturity,coupon,yield,2,0)` | dc-3gx6, dc-j9k.50 |
| 13 | `DSUM`/`ABS`/`VLOOKUP`/`IF` chain yields an error where Excel saved a number (sally_beck Summary_Report) | ~560 | 2 | follows group 5 | dc-r526 |

Add-in functions (`XLL.*`, `XLUDF.*`, `BLP`, `BLPH`) are not in any group: the saved value comes
from an add-in that is not loaded here, so it is not a comparison. Owner: add-in hosting
(dc-6332.10).

## Tooling observation

The corpus run keeps about 20 MB of JSON per workbook (240 MB for the biggest), nearly all of
it per-cell records. A full Enron run needs about 190 GB unless thinned; there is no lean mode.
Bead dc-9pyu. Report classification of manual-calc workbooks: dc-qahd.

## Evidence

Raw outputs: R2 `dnacalc-corpus/runs/dna-koderbot/20261111-enron-saved-cache/` (per-workbook
archives, shard summaries, offender files, provenance manifest, aggregation scripts, with file
and byte counts recorded in `COUNTS.txt` there). Not committed.

## Open

Live agreement is unmeasured: only an Excel capture of these files would give a pass. The 19
timed-out workbooks and the 3 M undecomposed not-run cells need a rerun with a thinner output.
