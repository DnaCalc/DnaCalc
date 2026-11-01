# P03 result

Excel 16.0.20527.20088 x64, CalculationVersion 191029, Windows/AMD Ryzen 7 4800H, safe mode.
Eleven conditions x ten fresh workbooks, six VBA conditions x ten with an object-aware logger,
and ten corrected unused-argument trials: 180 workbooks, zero probe faults, owned-process-zero cleanup.

Native ordinary `BU` and thread-safe `BU$` callbacks received status 64 (`xlretUncalced`),
logged the unwind, and entered again with a successful coercion to 11. In all ten reversed-area
trials each registration had this pair. Thread-safe first calculation also had the pair in all ten;
worker thread IDs differ. Keep `ReevaluationReason::XllUncalcedUnwind`, a new semantic ordinal,
and effects executed before the unwind. Physical retry must not hide these native entries.

Macro-equivalent `BU#` read 11 successfully in the dirty reversed-area condition without status 64.
It was invoked on clean F9 in all ten trials; `xlfVolatile(false)` suppressed those clean F9 calls.
The reference selected inside a callback is a separate `B#` lane, with its own native trace.
Registration privilege and volatility must remain separate properties.

VBA Range arguments remained Range objects: their observed Value2 could be Empty before the
producer and 11 on re-entry. A Double parameter received 0 then 11 in a reversed-area calculation;
the expression argument B1+0 could instead wait for the producer. Null arguments were not observed.
Do not describe every Empty Range value as an Empty Variant argument. Full/Rebuild and same-value
rewrites caused fresh entries; clean F9 did not invoke the nonvolatile VBA controls.

`raw-native-and-vba.tsv` retains the original complete run. Its generic VarType logger could query
a Range's default value; `raw-results.tsv` repeats the six VBA conditions with explicit object
typing (vt=9/TypeName=Range) and exactly one explicit Value2 read in LateRange/LateVariant.
The unused-argument line in those earlier runs still called VarType directly, which queried the
Range's default value. `unused-raw-results.tsv` repeats ten cases after guarding IsObject and
logging vbObject directly. These corrected trials log vt=9/Range and enter once per tested stage,
including dirty reversed scope, with no warm-F9 call. The earlier unused-argument re-entry must
not be used as evidence for a function that never reads its reference. All old records are retained.
Native records are independent of that VBA logger. Kind 99 marks the stage numbers listed by
`native-summary.json`; 1=entry, 2=coerce status/value, 3=unwind, 4=exit, 6=volatility request, 7=Evaluate.
The value/bits fields are meaningful only for numeric/Boolean/error operand types.

Build with `build-native.ps1`. This is newly written measurement code against the documented
[C API types](https://learn.microsoft.com/office/client-developer/excel/data-types-used-by-excel) and
[registration privileges](https://learn.microsoft.com/office/client-developer/excel/xlfregister-form-1),
not an imported SDK implementation. RegisterXLL loads only this local image; no registry was changed.
Only the tested arrangements are established; one call does not prove every arrangement single-entry.
