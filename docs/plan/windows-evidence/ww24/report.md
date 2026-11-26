# ww24 Windows Excel answers (round 1)

- Excel 16.0.20625.20108, AMD Ryzen 7 4800H, private desktop via on-private-desktop.ps1; seven probes (a-g), zero windows on the user desktop each time.
- Evidence: docs/plan/windows-evidence/ww24 (probe sources, observations.jsonl, saved XML parts, bank-rows). status.tsv has 16 visited rows; bead-comments.tsv carries the same text.
- Measured: paste widths, CF merge, paste 7/11/12 variants, cross-workbook Copy/Move and group Move, protection Allow* matrix, UserInterfaceOnly, ListObject/filter/insert on protected sheets, password kinds and saved sheetProtection, AdvancedFilter in place after reopen, Unique subset, blank CopyTo headers, Calculate across books, Close with BeforeSave Cancel, style reads, Interior patterns, Activate/Select group, Points/data label/ChartObject name and geometry.
- bank-rows/ holds 43 differences in 8 witness sets, mostly object-model text answers against typed refusals in the product.
- Stalls: Unprotect with omitted password and re-Protect of a passworded sheet open a modal prompt the job could not dismiss; two probes timed out (rb.ps1 stopped only the recorded pids). Activation events did not fire in the hidden window (activation log empty), so event order (dc-oxh8w.50) is unmeasured.
- Open, not measured: AutoFit table (dc-igr0p), PageSetup/HPageBreaks (descriptor needs a build), dc-2g5fn VBA project after sheet delete/move, dc-j9k.5 GAMMA retry, CONFIDENCE.NORM/T.TEST/F.TEST sweeps (broker, Linux), dc-oxh8w.41 VBIDE, descriptor regeneration items (builds).
- Waiting for the owner: the 300 internet workbooks (isolated Excel VM), alternate locale and ANSI code-page profiles (parked), legacy formats that need a File Block or Trust Center change.
- No HKCU keys created, no Rust or .NET builds, broker untouched.
