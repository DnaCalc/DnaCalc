# Foundation

Start with [the Foundation history](history.md) for the decisions that led to the
Excel-only Rust workspace. The original documents and their revisions are in
[Foundation at `4736b348`](https://github.com/DnaCalc/Foundation/tree/4736b348a2f88864e17364de44a430095cce9f5e).

| Material | Location |
| --- | --- |
| Program chronology, retired proposals and decision sources | [History](history.md) |
| Empirical results and limits from the original Excel campaigns | [Evidence notes](evidence.md) |
| File paths, blob IDs and path history | [Inventory](inventory.tsv) |
| Dispositions, batch log and source offsets | [Classification guide](classification-guide.md), [classified inventory](classified.tsv), [batch log](batch-log.tsv), [preliminary triage](triage.tsv), [pending batches](pending-batches.tsv) |
| Current architecture, contracts and implementation work | [Architecture](../../architecture/README.md), [bulk-port plan](../../plan/BULK_PORT_PLAN.md), crate contracts and the tracker |
| Current source guidance for file formats and function research | [Format references](../../FORMAT_REFERENCES.md), [research sources](../../plan/RESEARCH_SOURCES.md) |

The extraction structure keeps the chronology and provenance together here. A reviewed
observation that changes a current rule belongs in the relevant architecture or
research document with a source link and a product-route witness. The inventory
points to run outputs, binaries, workbook fixtures and publisher mirrors in place.
Repository parking belongs to the owner in dc-zuwqx.5.

## Extracted Foundation material (`dc-zuwqx.4`)

| Extracted document / update | Foundation sources (`classified.tsv` `migrate`) |
| --- | --- |
| [Formula entry observations](../../research/formula-entry-2026-03-02.md) | `EXCEL_FORMULA_LANGUAGE_CONCRETE_RULES.md`, `EMP-0001`, `EMP-0002` |
| [Formatting spec constraints](../../research/formatting-spec-constraints-2026-03-05.md) | `FORMATTING_FORMAL_FINDINGS.md` |
| [Annuity exactness witnesses](../../research/annuity-exactness-2026-05-20.md) | `EXACTNESS_HIGHER_LEVEL_ESCALATION_CANDIDATES.md` |
| [Display vs machine numeric boundary](../../research/display-vs-machine-numeric-boundary-2026-05-20.md) | `EXCEL_DISPLAY_VS_MACHINE_NUMERIC_BOUNDARY_NOTE.md` |
| [Numeric cutoff and root cancellation](../../research/numeric-cutoff-and-exactness-2026-04-27.md) | `EXCEL_NUMERIC_CUTOFF_AND_EXACTNESS_THEORY_NOTE.md` |
| [Separator-sensitive formatting](../../research/separator-formatting-2026-05-20.md) | `EXCEL_SEPARATOR_FORMATTING_THEORY_NOTE.md` |
| [Worksheet text casing](../../research/text-casing-2026-05-20.md) | `EXCEL_TEXT_CASING_THEORY_NOTE.md` |
| [Excel calculation probes](../../architecture/EXCEL_CALC_PROBES.md) §1 | `EMP-0004` (P26), `EMP-0005` (P05), `EMP-0007` (P28), `EMP-0011` (P25) |
| [Public corpus sources](../../plan/corpus-sources.md) | `EXTERNAL_CORPORA_INDEX.csv` (`pbartxl`, `formulas.info`) |
| [VBA validation](../../plan/VBA_VALIDATION.md) §1 | `reference/test-corpus/vba/packets/` pointer |

Open on `dc-zuwqx.4`: Windows Excel comparison of the April `/2` and `POWER` underflow formulas against the November `16.0.20527` computed-subnormal case in [numeric-cutoff-and-exactness-2026-04-27.md](../../research/numeric-cutoff-and-exactness-2026-04-27.md).
