# Witness bank tiers

**Proposal for dc-jkf7s.** Keep the current pinned suite, add a reproducible active view of the
Excel evidence, and keep archived observations replayable from CalcFunc-Evidence. Start with ISEVEN
as the low-risk trial. Use ERF.PRECISE as a high-risk control for the time and coverage comparison,
keeping it full while its historical discrepancy identities are unresolved. Adoption and archive
eligibility are owner decisions.

## What runs when

| Tier | What it contains | When to run |
|---|---|---|
| Pinned | The checked-in portable WitnessSets. Aim for roughly 120 cases per function and preserve intentional exceptions where extra cases exercise distinct behavior. | Every lane check that touches CalcFunc or its dependencies. The browser suite exercises the same pins in the sweep. |
| Active | Every known CalcFunc discrepancy, every conflicting Excel observation, every protected edge row, and a stable sample of ordinary inputs. Keep the whole function active while it has failures or the archive prerequisites are unverified. | Judge the functions reached by an edit, including aliases and shared-kernel callers. Judge the entire active view in the periodic sweep. |
| Archive | Eligible ordinary observations left out of the active view. Keep their original files and provenance at immutable evidence revisions. | Replay monthly, before release, and whenever a relevant kernel, caller, coercion, transport, CPU path, or Excel build changes enough to invalidate the previous comparison. |

Routine replay uses the saved Excel answers. Re-read in Excel when version or CPU differences
matter, when two credible readings disagree, or when the recorded inputs or invocation facts are
insufficient. Start with the smallest distinguishing pair. Keep both readings and their provenance.
Score the applicable build and CPU separately where the evidence shows a difference.

## Selecting active rows

- Protect the exact input bits for either signed zero, subnormals, infinities, NaN payloads, DBL_MAX
  and its near neighbors, and the integers immediately around ±2^53. Keep typed input paths for
  text, logical, error, empty, arrays and references, and keep selected spill cells. Add
  function-specific boundary classes before archiving ordinary rows, for example truncation and
  parity transitions for ISEVEN and tail saturation and underflow transitions for ERF.PRECISE.
- Protect every historical mismatch, including fixed mismatches, and every conflicting Excel
  reading. Preserve unread, refused, and excluded obligations in the active denominator until the
  underlying evidence or invocation issue is resolved. Keep a durable, append-only record of these
  identities. Import lane WitnessSets containing Excel-observed differences. Historical sources with
  incomplete identity or provenance keep the affected function in full until the discrepancy can be
  accounted for.
- Sample ordinary cases per function and predeclared class. Allocate 8 per class for simple discrete
  or exact operations, 32 per class for the default, and 128 per class for observed numerical
  sensitivity and risk such as inverse functions, multi-stage approximations, and iterative solvers.
  Use the lowest seeded hashes of a canonical identity containing function, exact typed arguments,
  selected result cell and invocation facts. Classes should cover argument arity, type, sign,
  magnitude and function-specific domain regions. For predicates, distinguish result categories such
  as even and odd. Keep the seed, classifier version, evidence revision, selected keys and source
  locators in a manifest. Environment-specific observations for a selected key travel together.
- Consider archiving only after at least 30 days of full-bank exactness, demonstrated by at least
  two successful full replays 30 or more days apart with comparable coverage and no intervening
  loss. Revalidate eligibility after an applicable input, model or environment change. Discovery of
  a failure promotes its identity permanently and returns the function to full active replay while
  investigated. Keep an evidence revision that can reconstruct the original union and demonstrate
  restoration before the first archive move.

Report matched, failed and total counts for each tier. Archive reports should show both the
archive subset and the combined full population. Use the full population for the full-bank pass
rate.

## Measurement method

The comparison uses the Linux dev-profile judge on kode-fi (AMD EPYC Rome, 3.7 GiB RAM), DNA Calc
source revision `50d8dd4a` and CalcFunc-Evidence `4ef2c244`. Runs use previously captured Excel
answers. The default full judge replays the AMD-compatible study set on this host. Intel-specific
study identities contributed to the protection lookup and their separate replay remains part of
dc-ymjf.7. The pinned suite was benchmarked directly from the working tree, and the full
per-function comparisons use the existing `witness judge --bank full --func F --report ...` path.
Timing covers the prebuilt judge's whole invocation, including discovery and I/O.

The all-at-once full judge exhausted the lane's 2,920,000 KiB virtual-memory allowance after 92.97 s
with 2,366,040 KiB peak RSS. For the full comparison, a disposable build split evidence loading into
eight disjoint function groups. It kept the existing selection, invocation and verdict path,
including per-function replay inputs and exclusions. Shard timing includes repeated evidence
discovery and trace output. It characterizes the runnable full replay on this host. A monolithic
timing still requires a larger-memory host. Run `cargo xtask witness judge --bank full --report
<path>` with the same evidence revision to get that timing.

The prototype applied generic edge protection to whole decoded rows. An ordinary row had finite,
non-edge scalar numeric arguments and no explicit result cell. Its class was argument count, number
of negative arguments, number of integral arguments, coarse maximum-exponent band, and result kind.
For each class it selected the lowest keyed BLAKE2b-128 hashes with seed `dc-jkf7s.1/v1`. Low sample
quotas were used for a small set of discrete or exact operations, high quotas for known numerical
differences and sensitive families, and medium quotas elsewhere. It marked current differences and
argument matches to known historical differences from retained study captures, pinned predictions
and the committed Excel differences ledger. The resulting view is a sizing preview. It can
understate active size when a historical report gives failure counts without row identities. Archive
eligibility needs the function-specific boundary map, exact historical linkage and replay record
described above.

A separate audit of 296 revisions of `witness-status.tsv` found earlier full-bank failures for 362
of 506 functions. ISEVEN had two structural failures out of 57,066 in `b6f89a83f8`. ERF.PRECISE had
367,196 failures out of 613,943 in `89759ffa3d`. These are observations about older banks. Counts
alone cannot identify the protected rows or prove that the historical and current populations
coincide. The conservative sizing scenario holds each function with a current or historical failure
in full pending an identity audit. The ordinary-size estimate is useful for comparing costs. The
hold scenario estimates a budget after the dwell requirement is met.

The source inventory contains 2,793 candidate answer, oracle-cache, context, recapture and
study-discrepancy files. They occupy 319,133,440 bytes in their checked-out encodings and sum to
2,013,486,250 bytes before compression. The count includes candidates subsequently filtered by the
loader and study discrepancies for both CPUs. It excludes ancillary probes, repository history and
evidence compiled into the judge. The 281 pinned inputs occupy 7,696,160 bytes.

Raw and gzip view sizes represent the same selected trace records with the CalcFunc answer omitted.
They are comparable by row and include source and invocation on each record. They estimate replay
payloads, while the source files in CalcFunc-Evidence hold original observations, duplicates,
metadata, and alternate readings. Moving an identity to the archive view would save active replay
I/O with **zero bytes deleted** from the evidence repository.

## Whole-bank sizes and timing

| Population | Counted rows | Replay time | Decoded view bytes | Gzip view bytes |
|---|---:|---:|---:|---:|
| Pinned, current checked-in suite | 45,360 | 4.65 s | 7,696,160 on disk | — |
| Full, eight judge shards with traces | 4,794,193 | 892.31 s | 2,074,533,552 | 105,584,954 |
| Active, sampled preview in the same eight groups | 1,518,839 | 243.29 s | 737,660,644 | 41,514,611 |
| Active, estimate holding current and past failures in full | 4,276,531 | — | — | 93,843,956 |
| Archive, complement of the sampled preview | 3,275,354 | Monthly full replay above | — | 64,887,749 |

The full judge scored 4,763,244 matches and 30,949 failures over 4,794,193 rows and 506 functions
(99.3544% pass). The committed status baseline on the same total row count was 4,762,155 matches and
32,038 failures (99.3317% pass) across mixed code revisions. The pinned run scored 45,195 matches
and 165 failures over 45,360 rows (99.6362% pass). Its prior status was 45,194 matches and 166
failures.

The active preview had 1,487,890 matches and 30,949 failures over 1,518,839 counted rows (97.9623%
pass). The existing judge replayed 1,518,813 materialized rows. The remaining 26 exclusions retain
their full-bank failure obligations. The comparison verified the same observed failure count for
every function. Replay times exclude the separate selection and materialization step and sum the
per-shard wall times. They provide operating costs for these runs with different input layouts and
trace settings.

The preview would put 68.3% of full rows into the archive view. Holding each function with a
recorded failure in full leaves only 10.8% as potential archive candidates, before verifying the
exactness dwell or uncovered history. Use this second estimate for the initial capacity discussion.

The per-function inventory includes fresh verdict counts, historical peak failure counts, preview
and hold estimates, and view bytes. The historical peak is the largest reported failure count in a
single tracked snapshot of that function. A dash in the table above indicates an unmeasured
quantity. [Per-function measurements](witness-bank-tiers.tsv)

## Trial and adoption

Materialize the active view by function from its manifest so a lane can open only the functions it
touches. Filtering after loading the full bank would still pay the memory and I/O costs measured
here. Use the existing witness judge, with the active view supplying its input. Check the selected
keys, expected bits, invocation facts and count against full replay for the same evidence revision.
A monthly replay can use the full population and publish the archive subset alongside it.
Restoration is a manifest change using original source locators.

The pilot should first recover or explain ISEVEN's two earlier structural failures, validate ISEVEN
on a full replay, and inspect truncation, parity and the 2^53 transition before accepting its
reduced active view. Run ERF.PRECISE alongside it to check how a much denser bank benefits from a
larger ordinary sample. Functions with numeric failures, unread evidence or incomplete history stay
on full replay. Test the materialized active input against both debug and release judge
configurations before putting it in a scheduled run.

The quotas express how much repeated sampling to pay for. For scale, if 1% of a large ordinary class
regressed and sample membership were independent of the regression, 8, 32 and 128 draws would expose
it with probabilities of about 8%, 28% and 72%. A narrow numerical boundary can fall entirely
outside that sample. Preserve boundary cases explicitly and continue full replay.

## Individual function timings

| Function | Pinned rows | Pinned seconds | Full matched / rows | Full seconds | Active preview matched / rows | Active seconds | Full → active gzip bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| ABS | 120 | 0.28 | 1,302 / 1,302 | 5.45 | 206 / 206 | 0.06 | 23,231 → 4,167 |
| ACCRINT | 120 | 0.20 | 146,688 / 146,688 | 22.62 | 88,301 / 88,301 | 3.60 | 2,047,136 → 1,312,601 |
| BINOM.DIST | 120 | 0.14 | 70,343 / 71,359 | 9.78 | 70,343 / 71,359 | 2.50 | 1,057,228 → 1,057,228 |
| ERF.PRECISE | 120 | 0.21 | 618,437 / 618,437 | 34.35 | 677 / 677 | 0.05 | 10,839,771 → 16,066 |
| ISEVEN | 120 | 0.35 | 57,066 / 57,066 | 9.37 | 186 / 186 | 0.03 | 602,821 → 3,214 |
| SIN | 120 | 0.16 | 2,129 / 2,141 | 4.92 | 1,372 / 1,384 | 0.06 | 52,017 → 33,180 |

All 720 pins in these six runs matched Excel. Full and preview replay both preserved 1,016
BINOM.DIST failures and 12 SIN failures. Those two functions remain full under the eligibility rule,
even though the SIN estimate is smaller. Treating all typed-input paths as protected retains the
standard logical selector in BINOM.DIST and keeps its entire recorded population active. ACCRINT
also retains a large typed-input population. The savings depend on the actual inputs and coverage
policy as well as the row count.

The ISEVEN archive complement also replayed through a disposable WitnessSet. All 56,880 archive rows
matched in 1.42 s, and the archive view measured 600,560 gzip bytes. Its 56,880 rows plus the 186
active rows account for all 57,066 distinct full rows.

Each active wall time uses a separate disposable workspace containing only the selected WitnessSet
for that function. It uses the same judge and excludes the cost of deriving the set. The pinned
timings include discovery of the checked-in pinned directory, and full timings include discovery in
CalcFunc-Evidence. The prebuilt judge ran once per timing and other measurement jobs overlapped part
of the run, so the numbers give scale rather than precise speed ratios.
