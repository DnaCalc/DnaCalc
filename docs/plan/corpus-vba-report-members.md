# VBA table, chart and drawing member census (2026-11-14)

Static extraction and triage covered all 3,896 readable VBA projects in the census. Triage classified 3,368 clean and 528 suspect projects. No workbook or macro was executed. Member counts are case-insensitive source occurrences after removing apostrophe and `Rem` comments; the full per-project TSV and top-20 project summary are in R2 at `dnacalc-corpus/meta/vba-report-members-20261114/`.

## Most common members

| Member | Projects | Occurrences |
|---|---:|---:|
| `ListObject` | 92 | 1,185 |
| `TextFrame` | 53 | 1,522 |
| `SeriesCollection` | 50 | 784 |
| `Shapes.AddShape` | 47 | 210 |
| `ListObjects.Add` | 45 | 72 |
| `ChartObjects.Add` | 29 | 54 |
| `QueryTables` | 27 | 55 |
| `SetSourceData` | 23 | 39 |
| `AddPicture` | 16 | 37 |
| `Shapes.AddTextbox` | 15 | 21 |

Occurrences are syntactic counts and do not establish runtime behavior. Fixture candidates below are restricted to projects classified clean by static triage.

## Triage-clean candidates using table, chart and drawing members

Only two clean projects have at least one reference in each of the table, chart and drawing groups; they are listed smallest first. The hashes and set names identify the source workbooks without copying any VBA source into the repository.

| SHA-256 | Set | Bytes | Table uses | Chart uses | Drawing uses |
|---|---|---:|---:|---:|---:|
| `9a9a5595a58b0ed1da54566dc5bea1acc78a54e02b3fb3aba9f2e6df9090a3b1` | `new-zenodo` | 1,681,631 | 69 | 9 | 56 |
| `beec3fd33e2d871e132a12a35cbb3d4a526003e91bf5fa2314f4785ba1b98b84` | `new-zenodo` | 2,238,716 | 53 | 14 | 5 |
