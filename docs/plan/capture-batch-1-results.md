# Capture batch 1: first Excel answers (partial)

Run 2026-11-12 night, this Windows machine's Excel (own sessions on private desktops, 3 to 8 sessions,
macros force-disabled, dc-vq5b), compared with DNA Calc on koderbot (`cargo xtask corpus --no-capture`,
debug build). **This is a partial result: 289 of the 486 files were captured** when the run was stopped
(usage limit); the remaining 197 are not captured, not failures of the comparison. Captures are resumable:
re-running `cargo xtask corpus <dir> --capture-only --resume --cache <cache>` skips the 289 done.

## Capture

| Item | Value |
|---|---|
| Files captured | 289 of 486 (134,577 formula cells of 1,907,594 listed) |
| Not captured | 197: the large files (above about 6,000 formula cells, 0.25 s per cell measured, dc-53r6), 17 refused captures, a few timeouts |
| Timeouts | 8 at 900 s in the first pass (large files; those files were then queued in size tiers with longer limits); 2 at the tier limits |
| Capture refusals (about 40 of about 190 attempted after the tier restart) | SpecialCells failure 16, formula Excel reports that the package lacks 10, session baseline refusal 9, native object model not exposed 2, spill state unreadable 2, no receipt 1 (dc-qjnu) |
| Macro policy | every capture records `automation_security=3` (was 1), `macro_policy_requested`, `update_links`, `data_refresh`, `calculation_file_mode`, `calculation_full_rebuild` |
| Isolation | the guard of the private desktop recorded 0 windows on the owner's desktop and 0 foreground hits (`chain.guard.json`) |
| R2 | `dnacalc-evidence/excel/capture-batch-1/20261112/`: 1,259 files, 293 MiB (captures/, raw/ worker results, receipts, file list); `rclone check --one-way`: 0 differences |

Files are named by hash; sources are in R2 `dnacalc-corpus/sources/{enron,libs}/20261111/` (the hash list is
`capture-batch-1.md`). Captures stay out of the repository.

## Comparison (DNA Calc versus live Excel), 289 captured files

Pass = matched Excel; fail = mismatched + refused + failed or missing.

| | Cells |
|---|---:|
| Formula cells | 134,577 |
| Pass | 130,849 (97.22%) |
| Mismatched | 3,081 |
| Refused | 103 |
| Failed or missing | 544 |

The denominator covers only the captured files, so it is not a batch-wide rate. 3,153 of the 3,728 failing
cells are three capture or compare problems, not calculation: 2,604 cells "capture failure at stage reopen"
(2 workbooks, dc-ljie), 516 cells of one workbook whose comparison exceeded 600 s (dc-n2st), 33 with no
evaluator evidence.

## Root causes

| Group | Cells | Bead |
|---|---:|---|
| CORREL last bits (313 at 1 ulp, 84 at 2-16 ulp, 41 within 15 digits), 4 workbooks | 438 | dc-wli1 |
| Statistical functions 1-16 ulp (BETA.DIST, F.DIST*, T.*, CHISQ.INV, NORM*.INV, LOGNORM.INV, HYPGEOM.DIST, NEGBINOM.DIST, CONFIDENCE.NORM, IRR, SUBTOTAL) | about 25 | dc-bc1o |
| Error semantics: infix + (6), TIMEVALUE (2), REPT (6) | 14 | dc-4oy5 |
| Capture failure at stage reopen | 2,604 | dc-ljie |
| Comparison over 600 s (one workbook) | 516 | dc-n2st |
| Refusals owned by existing beads: FORECAST.ETS.CONFINT 44 and FORECAST.ETS 22 (dc-j9k), TEXT 25 (dc-uar), COUNTIFS 4, FORMULATEXT 4, defined name not compiling 4 (dc-10y) | 103 | existing |
| Capture throughput and refusals | n/a | dc-53r6, dc-qjnu |

## Saved versus live, in Excel itself

Values the file saved for its formula cells (read from the package before Excel started, `input_saved`) against
the live value after `CalculateFullRebuild`, 284 files, 134,534 saved cells:

| Result | Cells |
|---|---:|
| Identical (numbers bit-equal to the parsed saved text, text equal) | 122,618 |
| Equal within 1e-12 relative | 104 |
| Value differs | 4,793 |
| Type differs | 28 |
| Saved a number, live an error | 2,929 |
| Saved an error, live an error (codes not compared) | 3,621 |
| Logical (not compared) | 441 |

Volatile functions (TODAY, NOW, RAND) account for part of the differences; the rest is unclassified (dc-8txy).
The analysis script is not committed; it reads `input_saved` and `cells` of each `raw/*/result.json`.

## Not done

197 files not captured; no per-function breakdown of the saved-versus-live differences; the compare ran in
debug on a loaded host; macro execution for listed triage-clean files is not wired (none in batch 1).
