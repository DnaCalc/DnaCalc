# x18 exact RTD protocol (dc-6332.12, answer 13 B.9 step 4, C.5, D.14)

Excel 16.0.20527.20150 x64, private desktop (every guard: 0 windows, 0 foreground hits), host
`dna-calc-desktop --engine` at the commit that carries this file. Evidence outside the repository:
`C:/Work/DnaCalc/codex-runs/rtd2` (`e0` the Excel and host captures of the four RTD conditions,
`e1` the x5call condition, `h3`/`h5` the host captures compared against them, `meas3` the
numeric-topic measurement, scripts `cap.ps1`, `full.ps1`, `regress.ps1`).

## 1. Numeric topics through `xlfRtd` (N6): predictions, then Excel

`fixture/native/owned.cpp` gains `X5.RtdCall(progid, server, topic)`, registered `QQQQ` beside the
controlled numeric server: it forwards its three operands unchanged to `xlfRtd` (379). The job
`numeric-topics/measure-job.ps1` counts ConnectData calls per case (the server logs each call with
Excel's topic id to `rtd.tsv`). `numeric-topics/predictions.txt` was written before the run;
`numeric-topics/result.json` is Excel's answer. All 13 predictions held:

| Case | ConnectData calls | Shows |
|---|---|---|
| callback number 1 and callback text "1" | 1 | 1, 1 |
| callback number 1 and typed `=RTD(...,"1")` | 1 | 1, 1 |
| callback text "1" and typed `=RTD(...,1)` | 1 | 1, 1 |
| callback TRUE and typed "TRUE" | 1 | 1, 1 |
| callback 0.1+0.2 and callback "0.3"; 1E+20 and "1E+20" | 1 each | 1, 1 |
| callback text "t" twice | 1 | 1, 1 |
| callback error topic NA() | 0 | #N/A (status 0) |
| callback omitted topic | 1 | 1 |
| callback ProgID number 1; unknown ProgID | 0 | #N/A (status 0) |
| callback topic from a cell holding 1, and text "1" | 1 | 1, 1, 1 |
| callback "a" and typed `=RTD(...,"a","b")` | 2 | 1, 1 |

The number 1 and the text "1" are one topic through the C API as through the typed function, so
the host has one rule: `calc_func::read_rtd_call` (typed `RTD()` and the `xlfRtd` callback both
call it; before, `callbacks.rs` refused a non-text topic). The comparison case
`fixture/x5-rtd-callback-topic-cases.json` enters `=X5.RtdCall(P,,1)+X5.RtdCall(P,,"1")`: one
topic, two native callbacks per entry. `verify.py --rtd-cases` (x5call) compares each callback's
topic operand and result between Excel and the host: verified, client protocol 6 matched, 0
failed. The host binary built before the change failed this case (deadline: the callback was
refused, the formula never showed 84).

## 2. No second entry for ConnectData's value; what is still different

Excel returns ConnectData's value to the xlfRtd call that connected and enters nothing again for
it. The host admitted that value after the step and superseded the work, throwing the staged
result and any suspended activation away: every connecting function was entered twice (the X5
UDF answered 1, 1, 42 where Excel answers 1, 42; `a&b` with two connecting calls was entered a, a,
b, a, b where Excel enters a, b). The work now adopts the admission in place
(`RecalcWork::adopt_connected_inputs`; the certificate repair accepts a read of a provisional
value the connector reports as pending, `RtdConnector::provisional_pending`), and one RefreshData
batch is one admission and one recalculation in the controller. `verify.py` expected the doubled
answer for the X5 UDF; that expectation was Excel-wrong and is now Excel's `1, 42`.

Host against the Excel captures of `e0` (same tracer and fixtures; the host is given the workbook
name Book2, as Excel's session workbook is named: that spelling alone failed every native entry
before). The original-condition client count moved from 12 to 11 because one row depends on the
timing below (a); every value, dependent and exact P07 certificate matches in all conditions:

| Condition | Client calls matched / failed | Native entries matched / failed |
|---|---|---|
| original eight managed cases, before (e0, host at d32641cf) | 12 / 14 | 0 / 26 |
| original, after (two host runs: h3, h5) | 11 / 15 both | 6 / 11 and 5 / 12 |
| delayed six cases, before | 14 / 9 | 0 / 23 |
| delayed, after (h3, h5) | 14 / 9 both | 11 / 4 both |
| x5 numeric native, x5 typed, x5call (new) | 6 / 0 each, all verified | not applicable |

What remains different is timing and chain order, not protocol: (a) Excel disconnects a replaced
topic after the next topic's UpdateNotify when the new function's work is short (original, 50 ms)
and before it when the old formula had completed (delayed): the order follows Excel's own
scheduling, it is the same in both Excel captures (`rtd1/m4`, `rtd2/e0`) but depends on the
elapsed time between the edit and the next calculation; (b) the Excel driver sleeps 500 ms
between observations, so a function whose observable completes at once is entered a second time
there after the RefreshData (the `MyStringObservable` row); (c) Excel calculates the dependent of
a shared topic before the main cell (its calc chain puts the newest formula first), the engine
row-major; (d) Excel terminates and restarts the server when the last topic disconnects between
two cases, the host keeps it while the next formula connects first.

## 3. The other controlled-server conditions at this commit

Fresh Excel captures (`rtd2/e2`, same tracer) against the host at the commit that carries this
file, `verify.py` with source retention:

- `x8-rtd-array-cases.json` (a 2x2 SAFEARRAY from RefreshData, shown as #N/A without a spill):
  verified, 6 client calls matched 0 failed, 2 native entries matched 0 failed.
- `x8-rtd-reuse-cases.json` (completed-topic reuse across edits of A1): fails the verifier's
  assertion "Completed-topic reuse did not preserve ID and advance the input sequence" (it expects
  the admissions 1,42,1,42; the host admits 1,42 only). The same failure occurs with the host's
  adoption of ConnectData admissions switched off, so it is not caused by this lane's change; it
  is recorded here and not fixed.
- The signaling condition (`x5-rtd-cases.json`, xlOil Python): not re-run; the Excel capture of
  the lane's xlOil Python runtime stopped at "xlOil built-in Python startup did not complete".
  xlOil Python through the page (5 of 5) and the framework host route (5 of 5) pass.
