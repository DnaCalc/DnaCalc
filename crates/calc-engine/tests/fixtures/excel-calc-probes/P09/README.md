# P09 result

The same `=SQRT(A1:A4)` text produces scalar 2 through Formula and a spill [1,2,3,4] through Formula2 at B2.
Formula form must remain in the semantic key and be set by every ingress. Fixed CSE arrays also need their extent;
equal values and formula text do not make them dynamic spills. SavedAsArray must retain its Null state.

Excel 16.0.20527.20088 x64, Windows 10.0.26200, AMD Ryzen 7 4800H, safe mode. `machine.json` and raw profile rows
retain settings and the COM-add-in enumeration limitation. No security setting changed. A1:A4 contain 1,4,9,16;
C1:C4 provide the corresponding source for Copy Destination. Ten fresh workbooks per ingress were measured in
`P09-final-safe`; the preliminary `P09-trial-safe` contributes another ten files. No ingress operation raised an
error and there are no fault rows. Formula errors produced by the expressions remain recorded outcomes.

| Ingress | After Full calculation | Form observations |
|---|---|---|
| Formula, FormulaR1C1, legacy Copy Destination | B2=2; B3:B5 empty | Formula getter omits @; Formula2 getter adds @; no array/spill |
| Formula2 with explicit @ | B2=2 | Scalar result; Formula2 retains @ |
| Formula2, Formula2R1C1, modern Copy Destination | B2:B5=[1,2,3,4] | Dynamic spill, HasArray=false |
| FormulaArray over B2:B5 | [1,2,3,4] | HasArray=true, HasSpill=false, fixed CSE extent |
| Legacy FillDown | B2:B5=[2,3,4,0] | Relative scalar formulas |
| Modern FillDown | B2:B4=Error2045; B5 starts [4,0,0,0] | Four array formulas compete for their footprints |

Immediately after copying only the modern anchor, B2 HasSpill=false and B2:B5 SavedAsArray=Null. Full calculation
materializes its four-cell spill and HasSpill becomes true. All ten copies show this transition. All ten modern
FillDown cases produce the recorded obstruction errors; the operation itself succeeds. Do not call this a failed
automation command or change the formula form to avoid the errors.

`saved-caches.json` records the 110 original packages before reopen. Dynamic B2 has cm=1 and `<f t="array"
ref="B2:B5">`; the legacy CSE form has the same array text/extent without the dynamic cell metadata. Legacy scalar
B2 has ordinary `<f>SQRT(A1:A4)</f>` and cached 2. Modern FillDown's live Error2045 is stored as `<v>#VALUE!</v>`
with cm/vm and rich-value error metadata; the plain cached error string is insufficient to recover its class.
The complete workbooks retain all package parts, and the later reopen cache manifest also extracts richData and
sharedStrings XML in addition to workbook, sheet, calc-chain and metadata XML.

`raw-results.tsv` includes Formula, Formula2, both R1C1 forms, typed Value2, HasArray, HasSpill, SavedAsArray and
spill extents before/after Full. `observations.json` indexes their rows. The reopen run selects the 100 primary
files in both Manual and Automatic after a controller has selected the mode, opens them read-only and closes
without saving. Its raw rows explicitly distinguish the opened workbook from the controller; getter/array flags
are recorded directly before the target's diagnostic macro. This is after-controller load, not first-file startup.
All 200 opens completed without fault rows. Their B2:B5 values, getter forms and array flags match the corresponding
post-Full source observations in both modes (2400 value/form/flag comparisons, zero differences). This includes
live Error2045 reconstructed from the modern FillDown packages' error metadata. All 310 retained packages and
their exact XML/hashes are covered by reopen-saved-caches.json; source files were opened read-only and not saved.

Replay the shared driver with this directory and Repetitions=10. Extract with inspect-calculation-probe.py, then
use Reopen with explicit `manual~filename|auto~filename` cases, Repetitions=1 and a different ArtifactPrefix.
`supervisor-raw.zip` retains the injected source and source hashes. The sealed OBSERVATION_ONLY expected-value
mismatch is intentional; compile/run/cleanup and actual faults are reported separately.

Contract: preserve form, CSE extent, implicit intersection and calculated spill metadata independently. File load
must preserve dynamic/rich-value metadata rather than deriving semantics from identical formula strings or one
cached error token. Other paste transports (clipboard formats, importers and a UI text paste) are not measured by
this Copy Destination control.
