# From Foundation to DNA Calc

Foundation began on 22 February 2026 as the planning, research and conformance home for
DNA Calc. In October the implementation consolidated into a Rust workspace and adopted
Excel as its sole calculation model. The intervening work explains several present-day
choices: the split between formula evaluation and workbook scheduling, the importance of
provenance on a published result, and the decision to test the same path the product runs.
The [source snapshot][source] has 79 reachable commits. Dates below distinguish when
a decision or observation was made from when it entered Foundation's Git history.

## 1. The starting architecture

The [first commit][first] laid out a spreadsheet with two independent engines, Rust and
.NET, expected to agree across the same protocol. The [charter][charter] gave a third team
responsibility for Lean proofs, TLA+ models, an OCaml oracle, and conformance packs. The
intent was to make agreement inspectable and to keep evidence tied to the semantics it
tested. The old plans describe the intended assurance stack, while the August investigation
documents the much narrower stack that actually existed.

Three boundaries organized the [architecture][architecture]. An operation envelope would
carry every persistent change. A document snapshot would hold authoritative inputs at
an epoch. Calculation deltas would convey derived state, including stale and pending
values. The epoch in a result was important because a workbook can have accepted a
change while calculation is still in progress. The same boundary was meant to cover
structural edits, external updates and macro mutations.

The project also anticipated preserving unknown file parts, explicit refusal on an
unsupported operation, traceability of changes and clean-room evidence for Excel
behavior. These ideas survived. The particular operation log, the second engine, and
the profile identifiers on documents belong to this design phase. Their presence in
a Foundation specification records the proposal that motivated later experiments.

## 2. Pathfinder before product

Foundation called the first round DnaVisiCalc. It was meant to exercise enough of a
spreadsheet to test the architecture before investing in broad Excel coverage. Its
scope included cell and name inputs, formula evaluation, dynamic arrays, structural
rewrites, deterministic external updates, and visible calculation progress. The
[scope-alignment note][pathfinder] identifies the transition from the proposals in
Foundation to functional authority in DnaVisiCalc's own specifications.

The intended progression then split proving grounds by the boundary under test. DNA
VbCalc would exercise VBA independently. DNA OneCalc would exercise one formula and
the function and evaluator seams. DNA TreeCalc would exercise dependencies and
scheduling across nodes. The proposed DNA PreCalc would bring the tree and grid
together, with DNA SuperCalc as a later refinement. The design questions and names
survived at different rates.

The [May stocktake][may] records exercised Round 0 implementation scope and an
outstanding formal exit. At that point it did not find a DnaTreeCalc repository. The
[20 May commit][may20] records the subsequent program change, and the [Foundation
README][foundation-readme] dates creation of DnaTreeCalc to that day. It also records
the move of its planning documents to the host repository. This is why host design
material and Foundation doctrine can describe different stages even when their files
look like parallel specifications.

## 3. Building an Excel reference and observation record

On 28 February the first [compatibility index][compat] collected public Excel
documentation alongside a function inventory. The [empirical run][empirical] drove
Windows Excel through a .NET probe. It retained formula inputs, scenarios, outputs
and executable identity. Different waves investigated coercion, dates, formula
admission, table and spill behavior, conditional formatting, volatility, reopen
behavior and RTD. They yielded both usable observations and observations whose
instrumentation or setup needed a second look.

The [reference format][ref-format] explains why a source mirror, its extracted
sentences, an automatically proposed conformance item, and a curated empirical
finding carry different weight. A candidate requirement can come from a sentence
with a precise spec anchor. The Excel result can still differ or depend on the
build or setup. The curated observation needs the scenario, the run, the build
and the evidence path. The archived [source bindings][bindings] and [findings
index][findings] connect these layers.

On 1 March the corpus pass combined spec-derived and empirical material in a
[worksheet-engine conformance reference][conformance]. A subsequent [formula
pass][formula] used grouped scenarios and manual-preparation reruns to refine
language rules. That history is evidence for specific inputs and conditions.
The corpus and specs also record uncertainties, which matter when reconstructing
why a proposed rule did or did not become an executable expectation. A compact
selection with the original caveats is in [the evidence notes](evidence.md).

## 4. The calculation core and its seam

The [formal-model consolidation][model] described structure, references, dependencies
and derived values as distinct layers. Persistent changes crossed them through explicit
operations. A scheduler could use a dirty closure or a more elaborate incremental
strategy while preserving stabilized results. The separation also made it possible to
discuss visibility as a scheduling hint without changing worksheet semantics.

The [DAG research][dag-research] and its [theory companion][theory] compared
topological and strongly connected component scheduling with dynamic topological
maintenance, self-adjusting computation and differential stream processing. The
baseline used deterministic scheduling and a single publishing authority. The
proposals for partitioned or speculative evaluation were conditional on equivalent
results, reproducible contention and the cost of the additional machinery.

Dynamic references made the publication boundary concrete. A formula can discover a
dependency during evaluation, after static binding is complete. A spill can create or
clear a region at the same time its anchor value changes. A coordinator reading those
changes independently may act on inconsistent topology. The [FEC/F3E redesign][fec]
described a session that produces value, topology and shape together on acceptance.
A rejected session yielded a typed reason and no derived bundle. The historical
design attached authority to the snapshot, formula and capability being evaluated.

March's [handoff][handoff] placed the editable evaluator seam in OxFml and the core
engine model in OxCalc, with Foundation retaining mirrors. OxFunc supplied worksheet
function and value semantics, OxFml owned binding, execution and the evaluator side
of the seam, and OxCalc owned scheduling and publication. Current boundaries should
be read through the DnaCalc contracts. The older mirror is useful for the reasons
those boundaries were drawn.

## 5. Replay, function evidence and proving hosts

The March [Replay Appliance proposal][replay] separated lane-native capture,
normalization, replay, comparison and witness reduction. It assigned reusable
mechanics to OxReplay and envisaged a DNA ReCalc tool. Lane adapters would decide
the meaning of source events and which rewrites preserved a witness. The spreadsheet
host retained the final verdict when parts of a run were missing or incomparable.
The primary distinction remains useful: two values can compare equal while their
provenance makes the overall run ineligible as a conformance result.

Foundation's probe fixtures grew substantially. The [5 April commit][corpus] records
a thousand single-formula cases for Excel and DNA comparison. Follow-on commits
added FFT, calendar and FILTERXML cases. Function-level probes were useful because
they could isolate a textual formula, argument type, and expected answer without
requiring every workbook feature. This also made corpus counts easy to misread as
evidence for the whole workbook path.

April's [numeric-cutoff note][cutoff] records another distinction. Excel could
rewrite a typed literal to zero at admission and could flush the result of particular
operations after admitting the literal. It also recorded a compensation on some
root-level additions and subtractions that disappeared when the same expression was
grouped or nested. The later measured computed-subnormal case is described in
[the evidence notes](evidence.md). The older observations remain useful as cases
with defined entry and calculation routes.

The motivation for a formula-only host survived the particular host ladder. By
August the [program investigation][august] reported that the original DnaOneCalc
development repository was parked and that the live formula proving surface was
Bench inside DnaTreeCalc. In the new repository the formula-only build is
**DNA Calc - Formula Explorer**, and the Explorer also appears inside DNA Calc.

## 6. The grid and the document boundary

The [June grid-impact note][grid] names a strict Excel grid profile for bounded
sheets, A1 and R1C1 references, table overlays, dynamic-array spill, and behavior
sensitive to hidden rows. It assigns the workbook grid to OxCalc, reference and
binding support to OxFml, and package access, fidelity and authoring to OxDoc.
The tree substrate was still expected to continue beside the grid at this point.

A useful example of the file boundary is the note's treatment of pivot parts.
Opaque preservation can keep bytes intact across a save. A sheet edit may also
move a report or change a source range, leaving those same preserved bytes with
outdated coordinates. The note calls for geometry-aware rewriting where supported,
or an explicit fidelity loss when preservation cannot keep the references valid.
This is why an unopened package part can still constrain editing. The current
pivot and document contracts determine what a present build supports.

The [August investigation][august] traced a working host route through mutable
workbook state, the OxCalc document context, OxFml and OxFunc. It cited a two-sheet
demo and 51 host-core test passes at that snapshot. It also found a missing
integration: the host did not yet use OxDoc to open and save workbook packages.
OxDoc and OxCalc each had part of the required interface. The host had to connect
them so the program could show that its live edits produced valid workbook bytes.

The solution was a small file-backed vertical slice, W011. The September account
records that DnaTreeCalc opened the fixture through OxDoc, changed A1 from 7 to 10,
recalculated B1 from `=A1*3`, and saved the result. The reopened file held the
formula and the cached value 30. It cites host-core, dispatcher, desktop and Excel
COM evidence. That result established one route through the layers. The file
boundary still needed broader tests for names, tables, format, unsupported parts and
other edit shapes. [W011 instructions][w011] · [September investigation][september]

## 7. The difference between a result and a cache

The September investigation opened with a census of the small workbook collection.
It counted 181 readable files, 174 identified as Excel-authored, with 481 formula
cells across the scoped set. All were single-sheet and the largest was 14,955
bytes. One formula-recognition file supplied 212 of the formula cells. Sixty-two
workbooks were in manual mode, and the one 1904-date workbook had no formulas.
There were no shared formulas, CSE arrays, data-table formulas, spill metadata
cells, or at-sign operator cases in the census. Appendix B is the historical
measurement and records how it was made. [Census and diagnosis][september]

Saved values made early offline comparison possible, but also exposed a specific
false pass. OxCalc's consumer path could reject a formula then publish its
file-cached value with provenance `FileCached`. An equality check against the
same cache would score a pass even though the runtime never evaluated the formula.
Manual-mode loading, shared regions and data tables could expose related gaps.
The investigation also documented a reference comparison whose status was
computed and discarded on the shipping route, and costly scheduler fallbacks
that prototype counters did not measure on normal host edits.

The Measured Excel campaign proposed a workbook differential with an explicit
calculated, mismatched or unevaluated disposition, followed by engine and file
work against that instrument. It also pointed out that saved decimal serialization
could lose bits, so agreement at that surface answered a narrower question than
live binary64 comparison. In the DnaCalc [bulk-port plan](../../plan/BULK_PORT_PLAN.md),
a live pass requires genuinely calculated output and a matching Excel
observation. Supplemental saved-value comparisons use their own population.
The denominator includes refusals, failures and any other unscored cells.

## 8. The Excel-only decision

The September campaign still proposed keeping two profiles over one engine. The
[October planning brief][oct-prompt] and [reviewed October plan][oct-plan] record
the decision to reimplement with Excel as the sole model. The tree reference
language and dual-profile design were left behind. The plan separated formula
compilation and evaluation, workbook scheduling, document package fidelity, and
host authority in one Rust workspace, with native and browser routes sharing
portable calculation code.

The plan also proposed one evaluator, one graph and one observation and verdict
lineage. The source audit had traced a separate scalar evaluator in OxCalc's R1C1
plan path and seen function semantics repeated there. Earlier work exposed the same
pattern at the consumer boundary: the parts that ran in a test harness could differ
substantially from those called on a normal edit. A single path makes it possible
to ask which code produced a given value and to focus the corpus measurement there.

The [12 October review][oct-plan] corrected source availability and pinned the
candidate repositories. It found OxDoc with existing preservation and authoring
work, and recovered the September diagnosis and the consumer source for the plan.
It recorded inspections and plan validation. Its source table, estimates and
acceptance graph have their original historical status. In particular, the planned
number modes and rollout dates in that bundle need to be read in light of later
owner decisions.

## 9. From staged rollout to bulk port

On 14 October the [new repository][dna-init] imported the reviewed plan and
tracker. [Foundation's same-day update][readme-transfer] directed readers to the
new home and to OxFunc-Evidence. The planning snapshot remains a useful record
because it identifies the code, evidence and architecture the transition started
from. The live tracker and the Rust contracts record what subsequently shipped.

The [23 October bulk-port plan](../../plan/BULK_PORT_PLAN.md) replaced the
per-bead rollout. The repository already had substantial new Rust code, but the
plan reported that only 51 of 518 functions had reached its catalogue. Mature
legacy implementations covered much more of the surface. The new direction was
to reuse those implementations in large coherent packages, preserve the library
boundaries and portable targets, and measure behavior in bulk through the
application path. It kept typed refusals and evidence provenance as explicit
results. Later refinements made the treatment of awaits, held values and file
caches visible in the corpus report.

The plan also kept the repository roles distinct: CalcFunc supplies the kernels
and metadata, CalcFml handles formula language and evaluator plans, CalcEngine
calculates the workbook, CalcDoc owns the package and model boundary, and CalcHost
composes authoritative document, calculation and application operations. Formula
Explorer exercises the formula surface without the workbook engine. Consult the
[current architecture index](../../architecture/README.md) for subsequent
decisions and the relevant crate contract for each runtime boundary.

## 10. Finding and using the sources now

Foundation's last commits addressed copied Microsoft summaries and the pages saved
during the February index run. Git records an initial removal, substitution of
original descriptions, restoration of the pages and descriptions, and a final
attribution update. The sequence is a useful reminder to check origin per artifact.
The repository licence covers Foundation's original work. Publisher mirrors,
embedded extracts and sourced workbooks each have their own rights and provenance.
Prefer the publisher's specific section when citing a format requirement, and keep
the Foundation run pointer when citing a particular observation. [30 October attribution][attribution]
· [Format references](../../FORMAT_REFERENCES.md)

The [inventory](inventory.tsv) makes it possible to recover an exact original by
path and blob, while the [classification guide](classification-guide.md) records
the snapshot, approach and current review coverage. The [October architecture
review's historical survey](../../architecture/review-2026-10/foundation-ideas.md)
links earlier source proposals to later design questions, including per-value
epochs, result status, incremental scheduling and reversible edits. The
[research-source policy](../../plan/RESEARCH_SOURCES.md) governs new Excel function
research. These pointers preserve the value of the original material while allowing
the implementation and its evidence to evolve in one place.

[source]: https://github.com/DnaCalc/Foundation/tree/4736b348a2f88864e17364de44a430095cce9f5e
[first]: https://github.com/DnaCalc/Foundation/commit/fcc6d4b
[charter]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/CHARTER.md
[architecture]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/ARCHITECTURE_AND_REQUIREMENTS.md
[pathfinder]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/notes/VISICALC_V0_SCOPE_ALIGNMENT_NOTES.md
[may]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/notes/PROGRAM_STOCKTAKING_20260503.md
[may20]: https://github.com/DnaCalc/Foundation/commit/b7dccb7
[foundation-readme]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/README.md
[compat]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/research/runs/20260228-130325-excel-compat-spec-index-pass-01/README.md
[empirical]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/research/runs/20260228-180047-excel-compat-empirical-pass-01/README.md
[ref-format]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/REFERENCE_SPEC_FORMAT_AND_CONFORMANCE.md
[bindings]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/conformance/excel-worksheet-engine/SOURCE_BINDINGS.csv
[findings]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings_index.md
[conformance]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/conformance/excel-worksheet-engine/EXCEL_CONFORMANCE_SPEC.md
[formula]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/research/runs/20260302-070309-excel-formula-language-pass2-pack-01/README.md
[model]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/CORE_ENGINE_FORMAL_MODEL.md
[dag-research]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/research/runs/20260305-201430-dag-computation-theory-deep-research-pass-01/outputs/05_deep_research_synthesis.md
[theory]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/CORE_ENGINE_THEORY_AND_ALTERNATIVE_PATHS.md
[fec]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/conformance/excel-worksheet-engine/model/fec-f3e/FEC_F3E_REDESIGN_SPEC.md
[handoff]: https://github.com/DnaCalc/Foundation/commit/3583a6d
[replay]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/REPLAY_APPLIANCE.md
[corpus]: https://github.com/DnaCalc/Foundation/commit/c2f57d0
[cutoff]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/test-corpus/workspace/monitoring/EXCEL_NUMERIC_CUTOFF_AND_EXACTNESS_THEORY_NOTE.md
[august]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/notes/PROGRAM_INVESTIGATION_2026-08-30.md
[grid]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/notes/GRID_EXTENSION_PROFILE_IMPACT_2026-06.md
[w011]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/notes/FABLE_ULTRACODE_WORK_INSTRUCTIONS_W011_2026-09-01.md
[september]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/notes/PROGRAM_INVESTIGATION_2026-09-10.md
[oct-prompt]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/prompts/calc-reimplementation-planning-gpt6pro.md
[oct-plan]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/plans/calc-reimplementation-2026-10/DNA_CALC_REIMPLEMENTATION_PLAN.md
[dna-init]: https://github.com/DnaCalc/DnaCalc/commit/7aac582e
[readme-transfer]: https://github.com/DnaCalc/Foundation/commit/5e4bf94
[attribution]: https://github.com/DnaCalc/Foundation/commit/4736b348a2f88864e17364de44a430095cce9f5e
