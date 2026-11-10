# x17 status 64 batch (dc-6332.11, answer 13 D.1 and D.2)

Fixture: the X1 Rust fixture with its opt-in status 64 probes
(`crates/calc-xll-fixture/src/fixture/uncalced.rs`, marker `uncalced-tests.enable`). Cases:
`crates/calc-xll-reference/fixture/x17-uncalced-cases.json`. Each group enters a producer
`C3 = MyUcLate(A1+4)` (`BB`) and a reader at B3, then edits A1 (8) and calculates
`Range("B3,C3")` with the reader listed first: P03's reversed-area order, which enters the
reader before its producer. `MyUcLog()` then returns the probes' log since `MyUcReset`.

## Predictions (written before the Excel run)

| Group | Reader | Predicted log after the range calculation | Basis |
|---|---|---|---|
| ord | `QU` | `read:64:untouched;late:12;read:0:num:12` | P03 (`BU`): 64, then a fresh entry after the producer; result operand untouched is a guess |
| safe | `QU$` | `safe:64:untouched;late:12;safe:0:num:12`; threads `worker;main;worker` | P03 (`BU$`); the thread placement inside Range.Calculate is a guess |
| macro | `QU#` | `macro:0:num:11` (cached old value, one entry) | P03 (`BU#` read without 64) |
| value | `QQ` | `late:12;value:num:12` (no entry before the producer) | answer 13 D.1 prediction |
| valuesafe | `QQ$` | `late:12;valuesafe:num:12` | as above |
| owned | `QU`, DLLFree return | `owned:64:untouched;free;late:12;owned:0:num:12;free` | guess: Excel frees the discarded result |
| parent | `QU` calling xlUDF(MyUcRead) | `read:64:untouched;parent:64:untouched;late:12;read:0:num:12;parent:0:str:0\|num:12` | guess: xlUDF passes the child's 64 up |
| mixed | `QUQ`, the cell as reference and value (added after run3) | `late:20;mixed:num:20:0:num:20` | D.1: the value argument makes the call wait |
| again | `QU`, more callbacks after 64 | `again:64:untouched\|coerce=64:untouched\|caller=64:untouched;late:12;again:0:num:12` | guess; against the documented contract |

The host before this bead (af6b75cf) would answer every non-`#` pending read with 32 and a
refused formula (harvest 13, 2.4 "Status 64").

## Results (Excel 16.0.20527.20150, x64, en-ZA, MTR on with 16 threads)

`excel/` and `excel-repeat/` are two runs of the final fixture (`codex-runs/x17-uncalced/run4`,
`run4-repeat`; X1.xll SHA-256 577E28B2...). Both agree on all 28 rows. Every guard recorded 0
windows and 0 foreground hits on the owner's desktop (`guard.json`).

Measured, against the predictions:

- **Status 64 writes #VALUE!.** xlCoerce of a reference whose producer is not calculated yet
  returns 64 and writes an xltypeErr #VALUE! into the result operand (predicted: untouched). The
  procedure is entered again after the producer, as a new entry, and reads the new value.
- **After a 64 Excel answers 64 again.** A second xlCoerce of the same reference and an xlfCaller
  in the same attempt both return 64 and leave their result operands untouched (sentinel bytes).
- **The discarded return is still freed.** A DLLFree result returned after a 64 goes to the
  add-in's xlAutoFree12 (`owned:64:...;free;...;owned:0:...;free`), as predicted.
- **xlUDF passes the 64 up.** A parent `QU` calling xlUDF for a child that gets 64 receives 64
  with #VALUE! from xlUDF; Excel then enters child and parent again after the producer.
- **`#` reads the cached value.** `QU#` entered first reads the producer's previous value with
  status 0, once; no second entry (as P03).
- **Value arguments are prepared before entry.** `QQ` and `QQ$` with the pending cell as a value
  argument are entered once, after the producer, with the new value; no entry before it.
- **A mixed call waits for its value argument.** `MyUcMixed(C3,C3)` (`QUQ`) is entered once, after
  the producer: the prepared value and the xlCoerce read both see the new value, status 0.
- **Range.Calculate is single-threaded.** Both entries of the `QU$` reader (the 64 attempt and the
  retry) and the producer ran on Excel's main thread.
- Earlier runs (`run1` to `run3`, outside the repository) used a producer that logged every
  call: Excel sometimes calculated the producer a second time with the same value, in different
  groups per run. The final fixture logs the producer only when its value changes.

DNA Calc's engine (`dna-calc-desktop --engine --cases`, `engine/`, this bead's build) against the
first Excel run: `verify.py --ordered-host` (`verify.json`): 28 of 28 pass (fail 0: mismatched 0,
refused 0, missing 0); value cases 18 of 18, ordered log rows 10 of 10; Excel repeat 28 of 28.
No before run of the old host was taken: its runner had no `range` calculation, and its code
answered a pending xlCoerce with 32 and a refusal (harvest 13, 2.4).

## Excel-DNA pair (`exceldna/`)

Answer 13 D.1's managed pair in a new fixture project (`fixture/X17Uncalced.csproj`,
`fixture/Uncalced.cs`, Excel-DNA 1.9.0, net48), cases `fixture/x17-exceldna-uncalced-cases.json`:
`ED_TRY_READ` (XlCall.TryExcel) and `ED_THROW_READ` (XlCall.Excel, catching XlCallException and
returning -999), each ordinary and `IsThreadSafe`, with `AllowReference=true`. Predicted: the
non-throwing call sees 64, the throwing one catches 64, and the caught -999 is never the cell's
value. Measured, two Excel runs identical on all 12 rows: exactly that (`try:64`,
`throw:caught:64`, then a fresh entry reading the new value). DNA Calc's engine: 12 of 12
(`exceldna/verify.json`).
