# Search for Excel's calculation programs

Owner direction, 2026-10-31; dc-j9k and dc-ymjf.4. This is a plan, not a search result.
The target is Excel's program: its operations, constants, branches and stores.
A better match count does not identify it. No fitted coefficients, residual cutoffs,
answer tables or switches chosen by disagreement belong in the implementation.
Published methods remain generic fallbacks until Excel's actual program is identified.

## Inputs and execution on this machine

- dna-firehorse: Linux x86_64, AMD family 23/model 1 under KVM, 16 logical CPUs,
  62 GiB RAM. Hardware x87 and the existing `soft-x87` backend are available.
  Excel/COM is on the Windows machine; this host can replay recorded answers.
- Freeze DnaCalc revision, source diff, CPU, compiler, numeric mode and evidence revision.
  Reuse `cargo xtask witness judge --bank full --trace <outside-repo>/application.jsonl`.
  `OxFunc-Evidence` at `0df452ec` supplies the bank; preserve input bits, omissions,
  argument types, selected result element, expected outcome, source and environment.
- Extend xtask's existing witness loader and calc-verdict encodings, rather than building
  another importer or oracle. Never ingest predictions or synthetic values as Excel answers.
  Retain disagreements by environment; missing, refused and unread rows remain failures.
- Candidate programs are research data interpreted outside the application path. A retained
  program must ultimately replace the existing calc-func kernel and run through `invoke`.

## Representation and elimination

Use a typed DAG: argument, published constant, arithmetic, elementary-function call,
explicit store, predicate, branch, fold and, later, recurrence/solver nodes.
Each arithmetic node specifies strict binary64 or an x87 precision/control word;
extended values remain extended until an explicit binary64 store or final publication.
Distinguish sharing an intermediate from recomputing it, and FMA from separate operations.
Constants carry their published source and loading rule: binary64 bits, extended ROM,
or an explicitly specified derivation. Do not optimize their bits against answer counts.
Branches require a mathematical/domain condition or independently distinguished exact
transition; a convenient boundary between regions where two approximations win is insufficient.

Enumerate a bounded grammar by size, association, instruction and store placement.
Do not apply real-number rewrites as floating-point identities: they generate alternatives.
Hash serialized programs, but check equality before deduplication. Prediction-vector equality
on a finite discovery set permits shared evaluation, not a claim of global equivalence.
For continuous nodes retain the full extended value and its precision/provenance tag.
Reject a candidate on its first exact-bit, kind, error, sign-of-zero or shape contradiction.
Persist the program, first killing witness and both outcomes; never rescue it by error size.
Zero survivors means the grammar or a frozen substrate is wrong: report the missing axis.
Multiple survivors call for inputs where their predictions differ, with exact controls and
adjacent-value probes around independently suspected branches. Freeze questions/predictions
before answers, then use the existing `witness read --inputs` route on Windows and cache them.
Hold discovery, fresh validation and previously assumed substrate controls separately.
Existing round-3/4 validation groups have been inspected and are not fresh validation.

## OxFunc material to adapt

Read-only source: [OxFunc 05196c39](https://github.com/DnaCalc/OxFunc/tree/05196c394f5400eb329c79b01b4a8d2bac7c228b).
The method is in `docs/SMART_SEARCH_AND_ACTIVE_LEARNING_LOOP.md`; the constraints are in
`docs/function-lane/W109_WALL_CLUES_LEDGER.md`. Read only permitted sources under
`RESEARCH_SOURCES.md`; no `smart-fuzzer/work` third-party source is needed.
Under `smart-fuzzer/tools/calc_graph_racer/` the reusable pieces are:

| Existing piece | Adaptation |
|---|---|
| `src/dsl.rs` | Serialized DAG, exact constants, control words, stores, predicates, folds, hashes. |
| `src/enumerate.rs` | Ordered association trees, store masks (16-node cap), model assignments (100k cap). |
| `src/eval.rs` | Binary64/Ext80 value flow and lazy branches; bind to DnaCalc's current Fpu interface. |
| `src/scheduler.rs`, `src/main.rs` | `race`, `distinguish`, `eliminate`, survivor files and first-kill records. |
| `src/bin/optree_foundation.rs`, `optree_search.rs` | Size-indexed value-vector banks and root interval joins for PMT's expm1 block. |
| `score_pmt_em_invert.rs` | PMT decomposition plumbing; use rounding preimages instead of a rounded inverse. |

The wall ledger records conflicting/stale hypotheses as well as useful constraints;
its percentage leaders and fitted coefficient proposals are not identified programs.
Do not port `score.rs`'s match/ULP ranking as a selection rule. `eliminate` currently skips
unparseable expectations: replace that with explicit unsupported/failed accounting.
Its witness schema lacks general typed outcomes, environment identity and selected elements.
`eval.rs` depends on OxFunc's `research-x87` API; adapt to current cleared DnaCalc operations,
never copy the old numeric substrate. Some transcendental nodes publish binary64 regardless
of the store flag, so continuous-entry variants need explicit semantics and parity tests.
The op-tree tool's approximate join windows and two-tree coverage mining are not proofs;
replace windows with exact rational rounding intervals, including ties and open endpoints.
Missing: a DnaCalc research interface for private Fpu operations, a bounded/checkpointed
enumeration command within xtask, canonical typed outcome adapters and reusable exact interval
joins. No SMT solver, global expression-tree enumerator or loop VM is installed by this plan.

## First four-hour round: PMT's final combine

Scope: the finite discount arm, excluding zero rates, nonfinite exponentials and exact
opposing balances. Freeze the current log1p/expm1 outputs as leaves; do not refit them.
The earlier certificate found 734 targets unreachable by any binary64 quotient followed by
the existing stored-PC64 product among 250,730 discount rows; a 54-bit quotient was sufficient
for those gaps, which does not identify Excel's precision. Include these witnesses first.
This round therefore admits a resident PC64 quotient through the final product, rather than
repeating searches that force every quotient to binary64. Keep growth-arm controls separate.

1. Minutes 0–60: adapt the minimal arithmetic DAG/evaluator and witness inputs; prove strict,
   stored-PC64 and continuous-PC64 semantics against the current native/software operations.
2. Minutes 60–105: build two numerator trees, `PV+FV*(1+em)` and `(PV+FV)+FV*em`, with
   `tf=1+r*type`; three combines, `((N/em)/tf)*r`, `(N*r)/(em*tf)` and `(N/tf)*(r/em)`.
   Eight arithmetic sites with three models each give at most `2*3*3^8 = 39,366` programs.
   Constants are exactly 0/1; there are no learned thresholds or coefficients.
3. Minutes 105–180: stream exact elimination on 32 discriminating discovery rows, then all
   recorded scoped rows. Use at most eight workers and 4 GiB total, with checkpointed IDs.
   Deduplicate verified identical vectors; record resource truncation as incomplete coverage.
4. Minutes 180–240: replay survivors with the software backend and all recorded PMT controls;
   export at most 64 diverse disagreement questions for Windows if several remain. If none
   remain, retain their first kills and investigate the frozen expm1/numerator scope next round.

Deliver the grammar hash, coverage, survivor/kill records, exact interval certificates and
unanswered questions on the bead. Even a surviving graph remains a scoped hypothesis until
independent probes distinguish it. No search, port of the racer, or Excel run starts here.


## Round 7 result, 2026-10-31

PMT remains unidentified. The expm1/store family has 1,980 helpers and all 60 ordered
four-leaf multiply/divide trees, with 81 strict/stored-PC64/resident-PC64 assignments
per tree: **9,622,800 programs; zero survivors** on the 237,660 FV=0 discount rows.
The 108,255 nonunit rows independently reject every program. The eight first-kill
counts are 7,144,140; 1,097,530; 872,528; 111,582; 327,296; 384; 25,204; 44,136.
Whole-family reachability leaves **11,932 answers unreachable**; individually reachable
answers are constraints, never application passes. Growth and FV-nonzero rows stay
separate. The nper=1 identity branch has 729 programs, zero survivors on 129,959 rows
and 36,448 whole-family gaps; this branch was run natively only.

Exact rational reverse-rounding intersections reject all **4,860 combine profiles**
even with an arbitrary common helper per rate/nper/environment group. The nonunit, timing-fixed
projection has real-empty intersections for every profile, each certified by one to
three original witnesses. This is a complete first-kill group proof; the stopped
cross-matrix exports are partial diagnostics. Native/software PMT runs have identical
kill counts and gap identities, but seven published output sets and 777 gap root sets
differ; neither backend equivalence nor identification of Excel's helper follows.

The remaining search time moved to Poisson's count-one PMF: its defining expression
has one scalar input and an independently specified normalization, without PMT's
annuity numerator and timing. The 26,043,360-program five-model mixed-build family
has zero survivors and 73 whole-family gaps among 4,232 count-one rows. The generic
11,610-program mass family has zero survivors and 4,008 gaps among 13,388 rows.
The separate normalization projection rejects all 54,432 profiles
by exact real-empty intersections for an unknown common count-only denominator,
using the first 64 build-20131 rows and the original three arithmetic models.
These are finite-family exclusions, not a claim about all Poisson programs.

A publication-policy error was found during review: Poisson preserves finite
subnormal results, whereas PMT refuses them. The Poisson replay and question packet
were regenerated with the actual registered adapter policy; initial Poisson output
sets and the first question draft are explicitly superseded. Four actual invoke
controls cover both aliases and both false-flag encodings. No answers were changed.

Build 20527's 88 tiny-mean readings leave 52,908 scoped count-one hypotheses in
the 1,585,920-program refinement. These are rejected on the mixed-build observations;
build, input region and argument encoding are confounded. There is no application
build switch and no kernel promotion. The 38 frozen unanswered questions, with predictions
separate, are in `/home/ubuntu/codex-runs/fresh-questions-r7/`; use the existing
`cargo xtask witness read --func POISSON.DIST --inputs requests.json` on Windows.
Omit `--new-only`: the packet intentionally includes controls that repeat old inputs.

Research source is committed as `b46bd4e` in DnaCalc-Research (program code `624edfd`).
The complete local `DnaCalc-Research-r7-final.bundle` stays in the round artifact directory. Evidence revision is `0df452ec63a8adcb0bf8a9a248dab11c49379136`.
Sources used are annuity algebra, the [NIST Poisson PMF](https://www.itl.nist.gov/div898/handbook/eda/section3/eda366j.htm),
recorded observations, the cleared Fpu
and the MIT OxFunc `05196c39` racer subset; no excluded distribution source was read.
Native application traces are byte-identical before and after: PMT 222,784 pass / 69,067 fail
of 291,851; Poisson 39,026 pass / 10,652 fail of 49,678, including four unread rows.
Full corpus remains 18,752 pass / 1,191 fail of 20,030; refusals and missing results
count as failures. No product kernel or evidence pin changes; no recorded answers changed.
Exact scope, resources, corrected-run paths and validation are recorded in
`/home/ubuntu/codex-runs/work-report-7.md` and
`/home/ubuntu/codex-runs/dna-firehorse-20261031-round7/`.

The final upstream pull to `e663103` adds only an unused error-spelling method in
the research build's guarded source paths. Its unchanged revision guard correctly
refuses a fresh build there. Numerical runs and questions retain the binaries and
source unit built before that pull, at `ec05eae`; the numeric sources are unchanged.
