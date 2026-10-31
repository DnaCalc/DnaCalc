# P01 result

Excel 16.0 build 20527, CalculationVersion 191029, Windows x64, AMD Ryzen 7 4800H; safe mode.
Ten fresh workbooks for each of 25 conditions (250 saved artifacts); all initial B1/C1 seeds were 2.

Calculating C1 alone returned the dirty B1 cache **2**, without invoking the secondary B1 counter.
B1 then C1 and the contiguous range returned **11**. Reversed `C1,B1` returned **Empty** on the
first observation and **11** on re-entry. Full/Rebuild returned 11. After a warm clean chain,
sheet/application calculation updated B1 but did not invoke the hidden observer, leaving C1=2.
Self-read returned numeric 0 on formula entry and Empty on the two forced calls, even when 17
was stored before replacing the cell's contents. Both reads around WorksheetFunction.Sum saw 2
in all 20 PeekTwice calls. No within-call advancement was observed.

Decision: retain `CachedMacroObservation` for the narrow cached-read case, no hidden dependency,
and no artificial self-cycle. Do not equate an Empty observation followed by re-entry with
one resumed invocation. This measures command/chain-specific behavior, not universal read ordering.

Evidence: `raw-results.tsv` is the exact returned VBA text; `saved-caches.json` contains the
unopened saved XML and SHA-256 hashes. `supervisor-raw.zip` preserves rejected startup/setup trials,
the accepted compile/run/owned-process-zero record, injected source, and owned UI observations.
The supervisor's `passed=false` is expected: this measurement descriptor expects OBSERVATION_ONLY,
not a fabricated behavioral answer. `capture.json` reports faults separately (zero).

Limitations: fresh workbooks share one owned Excel process; "cold" means a new workbook chain,
not a reopened file. Value2 snapshots are VBA object-model reads orchestrated over COM, not a
separate external-COM read interleaved inside a VBA invocation. P01's accepted run predates the
shared workbook/application event logger, so only the supervisor runtime event record is retained.
Safe mode suppresses normal add-in loading; AddIns.Installed is configuration, not proof of loading.
COMAddIns enumeration is unavailable in safe mode and is recorded as such. No setting was changed.
