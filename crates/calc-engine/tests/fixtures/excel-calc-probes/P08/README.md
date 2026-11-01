# P08 result

All 200 repeated cases observed the new pure values (B1/H1=1010, D1=1200), but callback order varied under
multithreaded calculation. PeekSeveral ran before PeekB1 in 37 of 120 multiworker trials and after it in 83.
With MTC off, PeekB1 ran first in all 60 trials; with one MTC worker, it ran first in all 20. Stable read values do
not make the effect trace interchangeable. Retain explicit observation ordering; a deterministic DNA policy is
not a claim that every Excel schedule has one unique order.

Excel 16.0.20527.20088 x64, Windows 10.0.26200, AMD Ryzen 7 4800H, safe mode. `machine.json` and per-case settings
retain the profile. Add-in installation flags do not prove loading; COM add-in enumeration is unavailable in safe
mode. No security setting was changed. The one-thread cases successfully set ThreadCount through VBA CallByName
(error=0, actual=1), then restored the prior ThreadMode. One-worker MTC still uses a calculation worker distinct
from the main VBA thread; it is different from disabling MTC.

`probe.bas` builds two 1000-cell chains in K/L, pure outputs B1/D1, a thread-safe identity XLL mirror H1,
PeekB1 at C1, and PeekSeveral at F1. The latter reads all three outputs without authored dependency edges.
The accepted seed is B1/C1/H1=1001, D1=1100. It is explicitly calculated and checked before edits; "cold" means
no additional clean F9 after that seed, not an uncalculated file or a fresh Excel process. Warm cases add two F9
commands; rebuild cases use CalculateFullRebuild for the tested command, others CalculateFull.

The 20 conditions combine producer/observer entry history, cold/warm/rebuild and MTC on/off, plus a different
active sheet, scrolled viewport/selection, lower chain position, and one worker. Each uses a fresh workbook and
repeats ten times in one owned process. All tested commands invoked each observer once; no tested read returned
the old value. Nominally identical multiworker conditions exhibited both observer orders, so the variation cannot
be attributed uniquely to one history or viewport setting. The seed calculation also limits conclusions about
entry history before a chain has been established.

`native-order.cpp` extends P03's ABI/logger with XPure (`$`) and a logger called by PeekSeveral. Native kind40 is
the pure mirror, kind41 the observed values and kind99 the stage marker (1=entry/setup, 2=tested command).
Worker IDs and exact order/value bits are in the native TSV files; VBA entry/exit/counters and events are in
`raw-results.tsv`. These logs instrument the callback boundary, not Excel's internal pure-cell schedule.

`P08-final-safe` is the primary 200-case run; `P08-trial-safe` adds 20 setup-control workbooks. All 220 workbooks
and their exact cached XML/hash records are retained in `saved-caches.json`. `capture.json`, `observations.json`,
`native-observations.json` and `supervisor-raw.zip` retain source hashes and raw supervisor evidence. The sealed
descriptor intentionally expects OBSERVATION_ONLY, so its expected-value mismatch is not a failed measurement;
compile/run are ok, worker cleanup is owned-process-zero, and there are no fault or unrunnable rows.

Build `build-native.ps1`, then run the shared `run-calculation-probe.ps1` with this directory, NativePath pointing
to native-order.xll, Repetitions=10 and SafeMode. Extract with the shared inspect-calculation-probe.py. The original
P03 native image is unchanged. Native TSV files preserve their original bytes in Git so recorded hashes survive
checkout.

Contract: the tested Full/Rebuild class consistently supplied new cache values. The general legality of running
pure work ahead of an effectful observation remains open; keep the contract's serial fences/reference policy and
record the observed MTC order variation in the compatibility profile.
