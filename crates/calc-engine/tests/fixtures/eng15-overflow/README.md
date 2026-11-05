# Eng15 rounding-overflow observations

This is the unchanged ready workbook and original 94-cell capture from
~/codex-runs/excel-results-eng15, Excel 16.0.20527.20150. Its physical input
SHA-256 is 603ce07db6e424a5bc457c8edc272440b283bf50d08f2ff6802046d69be3e149.
The ready workbook, rather than the intermediate generator input, supplies the
exact Z1=DBL_MAX constant. Every source formula, original Formula/Formula2
readback and immediate observation was checked against the capture worker.

All 425 delivered hashes verify. This capture reused owned Excel PID19412,
launch1, with fresh_required=false, Automatic calculation and iteration off.
The baseline had MTC enabled with 16 threads and VS15ExcelAdaptor connected.
It is not a fresh-process, MTC-off or strict-target claim. The separate display
trials retain Value2 unchanged at widths 8.43, 20 and 40; rendering parity is
unmeasured. Both failed display preparations remain in the external delivery.

Current comparison gives 76 matches and 18 failures: 15 typed refusals and three
mismatches. Refusals cover eight comparisons, six multi-number extrema and one
concatenation. ABS of the negative overflow has the wrong sign, and two TEXT
forms return #VALUE! instead of the recorded text. These remain counted failures,
owned by dc-j9k.19 and the existing operator/coercion work. No answers or kernels
were changed. The original 30 save/reopen differences remain capture failures;
79 calculated results lack strict-target certification. delivery-lineage.json
under the paired captures directory records source hashes and every nonpass.
