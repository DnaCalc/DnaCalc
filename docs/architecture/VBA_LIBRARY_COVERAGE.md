# VBA library coverage in corpus workbooks

Excel VBA projects have four standard libraries: VBA, Excel, OLE Automation
(`stdole`) and Office. A library reference records a binding dependency. Source
analysis is needed to determine which members a project uses.

## Current projections

| Library | Checked-in surface | Current behavior |
|---|---|---|
| Excel | 40 types and 2,916 member entries in `excel.json` | Values-only and workbook handlers serve parts of the descriptor. The VBA adapter still has private spreadsheet bindings. The shared binding work is dc-piw0.5. |
| Office | 21 types and 364 member entries in `office.json` | The COM add-in path supplies COMAddIns, a CommandBars subset, and ribbon handling. The descriptor also has drawing types. It has no FileDialog, DocumentProperties, Assistant or FileSearch type. The workbook VBA reference currently follows the compiler's registered-library route on Windows. On portable builds a compile that needs an unavailable library names it. |
| VBA | Compiler built-ins and CalcVBA runtime | The differential library suite and its concrete discrepancies belong to dc-q6brh. A compile can fail on an unsupported name or signature. Compare supported calls against the suite before assigning coverage. |
| OLE Automation | `stdole` recognised by the CalcVBA catalogue. No standalone stdole descriptor in `calc-object-model`. | Recognition permits library binding. It provides no inventory of usable StdFont and StdPicture operations in the workbook host. Measure their declarations, activation and member calls separately. |

Member counts sum the entries in the committed descriptor JSON, including
separate accessors. Per-route coverage needs a supported handler as well as the
descriptor entry. A COM lookup with no recorded member returns
`DISP_E_MEMBERNOTFOUND`. A known Excel member without a handler follows the
object model's typed refusal contract. See [the object-model design](OBJECT_MODEL.md),
[the COM dispatcher](../../crates/calc-com-host/src/dispatch.rs) and
[workbook VBA library resolution](../../crates/calc-vba/src/events.rs).

## Evidence available for ordering

The v5 reference census counted 3,901 readable projects, with stdole stored in
3,865 and Office in 3,839. VBA and Excel references are implicit. The last
completed reference-set comparison used the v4 denominator of 3,868 projects.

| References allowed | Projects within the reference set | Gain |
|---|---:|---:|
| VBA, Excel, stdole, Office | 2,214 (57.2%) | |
| Add MSForms | 3,513 (90.8%) | 1,299 |
| Add Scripting | 3,541 (91.5%) | 28 |
| Add VBIDE | 3,559 (92.0%) | 18 |
| Add MSXML2 | 3,577 (92.5%) | 18 |
| Add WinHttp | 3,594 (92.9%) | 17 |

These are reference-set counts. They do not establish that those projects can
make every call. [Reference census and provenance](../plan/corpus-vba-census.md)

The November 14 table, chart and drawing scan retains counts by project hash.
Filtering the retained TSV to `triage=clean` gives 3,368 of 3,896 unique hashes.
Of these, 86 use at least one of the 15 names counted. The most frequent
source names in that filtered set are below. A project's occurrences can
count in more than one row.

| Source name | Clean projects | Occurrences |
|---|---:|---:|
| ListObject | 21 | 301 |
| TextFrame | 20 | 1,228 |
| QueryTables | 19 | 36 |
| Shapes.AddShape | 17 | 87 |
| SeriesCollection | 16 | 256 |
| SetSourceData | 15 | 24 |
| ListObjects.Add | 9 | 18 |
| ChartObjects.Add | 7 | 9 |

The retained scan counts source spellings. Resolving receivers, unqualified
calls, local shadows, enumerators, default members and late-bound operations
requires additional analysis. In particular, a spelling shared by Excel and
Office cannot be attributed to either from these counts.
[Member census and R2 artifact location](../plan/corpus-vba-report-members.md)

## Measuring the missing coverage

Bead dc-3zb90.1 owns the per-member census. Use the existing SHA-keyed clean cohort. For each admitted workbook, obtain the
project through CalcDoc and decode the VBA part with the existing static
CalcVBA reader. Record the ordered references and distinguish resolved member
uses from ambiguous names and unsupported file formats. Include the parse and
binding omissions in the denominator. Aggregate only names, counts and hashes.

Join resolved uses to the handlers for the particular VBA host route. Count
each project once in the first reference tier that supplies all of its required
members. Rank missing members by distinct projects affected and file behavior
work with the relevant owner. Use Touchstone for observable behavior on owned
minimal probes. Use the type library for signature and layout facts. This
review used retained aggregate data and repository metadata, with no corpus
workbook opens or VBA execution.
