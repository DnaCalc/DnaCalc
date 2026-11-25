# Macro host member census

The retained November 14 scan has 3,896 distinct workbook SHA-256 hashes, of which 3,368 are
labelled clean and 528 suspect. Among the clean-labelled hashes, 86 use at least one of the
15 spellings the scan records. The counts below use only the clean-labelled rows and count a
hash once per spelling. The TSV and its summary were checked against the two retained R2 objects.
[Scan provenance](corpus-vba-report-members.md)

The clean label is the one in the retained scan. The later baseline reconciles conflicting triage,
excludes XLM and adds four Mendeley workbooks, giving 3,265 eligible hashes. Its aggregate output
does not identify the excluded hashes in the member TSV. The numbers below order candidates within
the retained scan. They are not counts of resolved VBA calls or of workbooks that a fix would run.
[Baseline cohort](macro-host-baseline.md#cohort)

## Ranked candidates without a workbook handler

The first column identifies a descriptor member to investigate. It is a candidate binding for the
recorded spelling. For example, a TextFrame occurrence does not identify which type owns it,
and an AddChart occurrence may be a local procedure. Each count therefore bounds how many of the
scanned workbook hashes a spelling could contribute to the corresponding candidate.

| Candidate descriptor member | Scanned spelling | Distinct workbook hashes | Occurrences |
| --- | --- | ---: | ---: |
| Shape.TextFrame or ShapeRange.TextFrame | TextFrame | 20 | 1,228 |
| Worksheet.QueryTables | QueryTables | 19 | 36 |
| Shapes.AddChart | AddChart | 2 | 9 |
| Shapes.AddPicture | AddPicture | 2 | 6 |

The workbook profile registers TextFrame2, which is a different accessor from TextFrame.
Shapes.AddChart2 also has its own handler. The VBA route must be checked at the actual call
sites before treating either spelling in the table as a blocker.
[Descriptor](../../crates/calc-object-model/descriptor/excel.json) ·
[Workbook handler registration](../../crates/calc-host/src/automation/report.rs)

## Other scanned spellings

| Spelling | Distinct workbook hashes | Occurrences | Candidate coverage |
| --- | ---: | ---: | --- |
| ListObject | 21 | 301 | Ambiguous. The spelling can name a type or a member such as Range.ListObject. |
| Shapes.AddShape | 17 | 87 | Handler registered |
| SeriesCollection | 16 | 256 | Chart handler registered |
| SetSourceData | 15 | 24 | Chart handler registered |
| ListObjects.Add | 9 | 18 | Handler registered |
| ChartObjects.Add | 7 | 9 | Handler registered |
| AddLine | 3 | 442 | Shapes handler registered |
| Shapes.AddTextbox | 2 | 2 | Handler registered |
| AddChart2 | 1 | 4 | Shapes handler registered |
| AddConnector | 1 | 1 | Shapes handler registered |
| ConnectorFormat | 1 | 10 | Shape handler registered |

## Repeated engine probe

The separate repository-fixture run found Application.Build and Application.Calculation refusals
in 258 copies of one engine probe. Each spelling contributes one distinct project in that run.
The probe is outside the retained scan above. The per-entry TSV keeps the original workbook and
entry counts, which describe execution attempts.
[Probe results](macro-host-baseline.md#per-entry-run-through-the-command-route-2026-11-25-dc-oxh8w1)

dc-3zb90.1 owns the source-level binding census on the reconciled cohort, including
ambiguity and unavailable-library counts, deduplication by project source across packages,
and the join to the VBA route. The retained aggregate cannot measure the four-library coverage
or the MSForms and Scripting increments. dc-3zb90 owns prioritizing behavioral work once those
calls are identified. The per-entry guarded corpus run remains with dc-gync0.
[Library census requirements](../architecture/VBA_LIBRARY_COVERAGE.md#measuring-the-missing-coverage)
