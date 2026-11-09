# Volatility as Excel does it: measured behaviour and what the evaluator must record

Date 2026-11-10. Measured by lane vol-1 (bead dc-hsa5.21) on Excel 16.0 build 20527, x64, Manual calculation,
MTC on. Probe ids are those of `EXCEL_CALC_PROBES.md` section 4; raw rows, readouts and tables are in
`crates/calc-engine/tests/fixtures/excel-calc-probes/volatility/` (README there). Input to R3a (dc-hsa5.20.2).
Every reading was repeated three times; none disagreed, except the per-call scripts (`HSeq`, `HuSeq`, which
depend on how many calls came before). "Measured" below means
read in those files; "not measured" means no probe covers it.

## 1. Instrument

A cell has two volatility facts in the saved file: a leading `PtgAttrSemi` token in its formula (`tok`) and the
always-calculate bit (`ca`; XLSX `ca="1"`, XLSB flag bit 1 of the cell record). A dependent counter
`Cnt(A<i>&"")` counts the recalculations of `A<i>` in a clean Calculate. On 12 formulas of known behaviour over
3 trials the counters and `ca` agreed everywhere, in XLSB and XLSX; `ca` is therefore a usable readout of
"recalculated at every Calculate", and every probe below reports both layers.
Two facts about the instrument itself:
- `ca` is also set on dependents of volatile cells (X7), so it is the whole volatile cone's bit, not the formula's
  own. The token is the formula's own.
- A user function cell whose only dependence is a bare reference argument (`=Cnt(A1)`) is not recalculated when
  `A1` is volatile (`instr`, X7 F3/F6; recalculated when `A1` is edited). Not explained; it is a dependent
  behaviour the evaluator does not model today. Concatenation (`Cnt(A1&"")`) makes the dependence normal.

## 2. The rule that holds: one flag, set by the formula's token, overwritten by ordered events (F2)

Per evaluation of a formula cell: the flag starts at S (the formula text contains a token-writing function) and
each event overwrites it in execution order; the value after the last event is the cell's `ca` bit, and the
cell stays volatile exactly while that value is true. DNA's current rule (sticky OR of S and the last request,
F1, `recalc.rs:2613-2619`) is wrong; F3 (flag set when a built-in runs) is wrong too.

| Evidence (P2, `p2`, `p2h`, `p6`) | F1 | F2 | F3 | Excel |
|---|---|---|---|---|
| `RAND()+VFalse()` (token, then request false) | vol | not | not | **not** |
| `VFalse()+RAND()` | vol | not | vol | **not** |
| `IF(TRUE,VFalse(),RAND())`, `IF(TRUE,RAND(),VFalse())` | vol, vol | not, vol | not, vol | not, vol |
| `NOW()+VTF()` / `NOW()+VFT()` (T,F / F,T in one call) | vol | not / vol | not / vol | not / vol |
| `VTrue()+VFalse()` / `VFalse()+VTrue()` | not / vol | not / vol | not / vol | not / vol |
| `VArgF(RAND())` (request inside a formula holding a token) | vol | not | not | not |

Also measured, all consistent with F2: `SUM(RAND(),VFalse())`, `VFalse()+IF(TRUE,1,RAND())`, a helper that
requests true on the caller's behalf (`VFalse()+VHelper()` vol, `VHelper()+VFalse()` not), three requests in
one formula (`T,F,T` vol; `T,F,F`, `F,T,T,F` not), and the same with XLL `#` functions (`HashFalse()+RAND()`,
`HFT()+NOW()`, `HashTrue()+VFalse()` and the reverse; `p2h`).

Evaluations start afresh: a function that requests true once (call 1) and nothing afterwards leaves the cell
volatile only until its next evaluation, which starts from S, sees no request and ends nonvolatile (cal1,
X1). Only a completed evaluation's events count (X4, section 6).

## 3. What sets S (the token): P1

Token present, syntactic, regardless of branch or use: RAND, RANDBETWEEN, RANDARRAY, NOW, TODAY, OFFSET,
INDIRECT, **CELL** (all 12 info types, literal or from a cell, with or without the reference, unknown type
text, range or reference-returning arguments: `p1ca`..`p1cf`), **INFO** (all 10 types, literal or from a cell),
**FORMULATEXT, SHEET, SHEETS** (`p1a`; the catalogue does not list these three). Not volatile: ROW, COLUMN,
ROWS, COLUMNS, AREAS, INDEX in every form tried (`INDEX(r,0)`, `r:INDEX(..)`, `INDEX(..):r`), ISREF, ISFORMULA,
SUBTOTAL, AGGREGATE, N, T, ADDRESS, CHOOSE, MATCH, VLOOKUP, XLOOKUP, HYPERLINK, TEXT, date functions,
SUMPRODUCT, LOOKUP, STDEV, TRANSPOSE. WEBSERVICE was not measured (it needs the network).

The token covers the whole formula text, executed or not (`p1b`, 34 formulas): `IF(TRUE,1,RAND())`,
`IF(FALSE,RAND(),1)`, `CHOOSE(1,1,NOW())`, `IFERROR(1,RAND())`, `IFNA`, `IFS`, `SWITCH`, `AND(FALSE,RAND()>.5)`,
`LET(x,RAND(),1)` with `x` unused, `LET(f,LAMBDA(RAND()),1)` (lambda defined, never called),
`(LAMBDA(x,1))(RAND())`, `MAP(.., LAMBDA(x,x*RAND()))`, `IF(C1=2,TODAY(),0)*0` with a false condition. A LAMBDA
body without a volatile function is not volatile. The token is not written for `!`/`#` registered functions,
for the run-time decisions below, or through defined names (section 5).

Run-time decisions, no token, `ca` set while the condition holds (`p1d`, `p1e`, `p1f`):
- **SUMIF and AVERAGEIF** when `sum_range` is smaller than `range` (`D1:D2`, `D1`, `D1:D1`, a range built by
  `D1:INDEX(D:D,n)` with n < 3 against a 3-cell range). Same shape, a larger sum range (`D1:E3`), a single-cell
  range, SUMIFS, COUNTIF(S), MAXIFS, MINIFS are not volatile. Decided per evaluation from the shapes the
  executed call gets: with `n` toggled 2, 3, 2 the cell was volatile, nonvolatile, volatile again; an untaken
  branch holding a mismatched SUMIF is nonvolatile.
- **A spill blocked by another cell** (#SPILL!): `ca` set, no token (`TRANSPOSE`, `SEQUENCE`, `TEXTSPLIT` into
  an occupied range). Free spills and `INDEX(r,0)`, `SORT`, `FILTER`, `EXPAND` are not volatile.
Both behave as events at execution (section 4): `SUMIF(mismatch)+VFalse()` is not volatile,
`VFalse()+SUMIF(mismatch)` is (`p6`).

## 4. Events, in execution order

1. **Registration default `!`**: true, applied when the call starts. A later request overwrites it: `BangHashClear()`
   (`Q!#`, requests false on every call) and `BangHuF(x)` are nonvolatile for every argument; `Bang()+VFalse()`
   is nonvolatile, `VFalse()+Bang()` volatile (`p3`, `p2h`). Not in the token; an untaken `IF(TRUE,1,Bang())` is
   nonvolatile. It also applies to `>EX!` async registrations.
2. **Registration default `#` with R/U parameter**: true, at call start, **only when a reference is actually passed
   in an R or U parameter** (`p4`). `Hu(C1)`, `Hu(C1:C2)`, `Hu(INDEX(..))`, `Hu(empty cell)`, `Hr(..)` are volatile;
   `Hu(1)` and `Hu(C1+0)` (a value) are not. Any R/U position counts (`Hub(C1,1)` yes, `Hub(1,C1)` no because
   position 2 is `B`; `Hbu(1,C1)` yes; `Hrr(1,C2)` yes). Q, P and B parameters never count; `Q#` with no
   parameter is nonvolatile; coercing the argument changes nothing. Same event order as 1: `HuF(C1)` (requests
   false) is nonvolatile for every argument, `HuT(1)` volatile, `HuTF(1)` not, `HuFT(1)` volatile.
3. **Requests**: `xlfVolatile(TRUE/FALSE)` and `Application.Volatile True/False`, any number per call; a call with
   a different argument per invocation behaves per invocation. `Application.Volatile` with no argument is true.
   Helper functions' requests count for the formula's cell.
4. **Name use** (section 5), **SUMIF shape**, **spill block**: events at execution that set true (name use: the
   name body's own flag, see 5).
No event clears a flag except a request of false. An untaken branch contributes no event.

**`xlfVolatile` is function 237 and Excel12 accepts it only in some contexts** (`p2x`, `p3`, `p4`, `x3`, `x5x`):
status 0 from a function registered with `#` called from a cell, and from any registration called outside a cell
(`Application.Run`, no effect on any cell); status 2 (xlretInvXlfn) from a cell call of an ordinary, `!` or `$`
registration, and from `>EX`/`>EX!`/`>EX$` entries; status 32 from the worker thread after the async return. So
`BangClear` (`Q!`, requests false) stays volatile: the request is refused, not obeyed. `$` with `#` was not
measured (DNA refuses the pair). DNA's `callbacks.rs` accepts it everywhere.

## 5. Names (P5, `p5`, `p6`)

A defined name's body has its own token (`RAND()`, `IF(TRUE,1,RAND())`, `INDIRECT`, `LAMBDA(RAND())` yes; a body
calling a user function or `=1` no). A cell using a name gets no token from it. Executing a name use is an event:
it sets true when the body's flag is true. Hence `=NmRand` and `NmRand*0` are volatile; `IF(TRUE,1,NmRand)`,
`IF(FALSE,NmRand,1)`, `IF(TRUE,1,NmVba)` are not; a name whose body is `VFalse()` leaves the cell nonvolatile;
`NmVF+RAND()` is nonvolatile (token first, the name's request later); `VFalse()+NmRand` volatile,
`NmRand+VFalse()` not; `NmInd+VFalse()` not, `VFalse()+NmInd` volatile. **`LET(f,NmLam,1)` is volatile although `f`
is never called**: binding the name evaluates it. Redefining the names without touching the cells
(`NmRand` to `=1`, back to `=RAND()`) changed the cells' volatility at the next Calculate, and the dependents
recalculated once on the redefinition: names are late bound, the fact belongs to the body, joined at each
executed use.

## 6. Persistence, commands, order, lifting, files

- **Calculate kinds (X1)**: a nonvolatile cell is not recalculated by Calculate or `Worksheet.Calculate`;
  `Range.Calculate` recalculates the target and leaves it dirty (target and dependents run again at the next
  Calculate); Full and Full-rebuild recalculate everything, requests are re-evaluated then (a function
  that now requests true becomes volatile only after one of these; one that now requests false ends the next
  Calculate). In Automatic mode volatile cells recalculate on an unrelated edit.
- **Entry and first calculation (X4, cal1)**: every formula is called once when entered and once more at the
  first Calculate; the events of the first Calculate decide the state (a request at that evaluation is
  honoured).
- **Abandoned attempt (X4, 3 runs)**: a reader of a three-level producer chain is evaluated twice in the first
  Calculate; a request made only in the abandoned attempt is lost, one made in the completed attempt counts.
  Only the completed evaluation's events are kept; never the abandoned one's.
- **Lifted calls (X2)**: with a Double-parameter user function lifted over a multi-cell range or array
  (`SUM(VElemD(C1:C3))`, `{1,2,3}`, 1x3 and 3x1, requests true and false mixed, all true) **no request
  is honoured: the cell is nonvolatile**; over a 1x1 reference the request is honoured; inside
  MAP/BYROW lambdas each call's request counts and elements run in ascending order, last wins (`MAP(C1:C3,..)`
  with requests F,T,T volatile, `MAP(E1:E3,..)` with T,T,F not). Repeated calls in one formula: last wins.
- **Outside a worksheet call (X5)**: `Application.Volatile` from a Sub, `Application.Run` or `Evaluate`
  raises no error and changes no cell; `Evaluate("VTrue()")` runs the function with no effect on cells.
- **Threads and async (X3)**: see section 4; `$` registered functions and async entries cannot clear or set
  by request; `>EX!` volatile; `AsyncLate` status 32.
- **File round trip (X6)**: saved, `ca` is written for requested, `!`, `#`+reference, SUMIF-shape and token
  cells alike (no token for the run-time ones). On open in Manual mode nothing is called; the first Calculate
  calls exactly the cells with `ca` (the reopened user-function cells included), none of the others. Automatic mode
  on open was not measured.
- **Dependents (X7)**: dependents of a volatile cell are recalculated every Calculate and carry `ca` in the
  file, to any depth, through ranges (`SUM(A1:A2)`); the bits clear when the source stops being volatile.
  Exception: the bare-reference user-function dependent of section 1.

## 7. What the evaluator must record and decide

Record per formula plan: S (any of RAND, RANDBETWEEN, RANDARRAY, NOW, TODAY, OFFSET, INDIRECT, CELL, INFO,
FORMULATEXT, SHEET, SHEETS anywhere in the formula text, including LET/LAMBDA bodies and untaken branches, not
through names). The catalogue's `volatile` flag becomes S, with those three added and the ByArgument cases
removed from it.
Record per completed evaluation, in order, `VolatilityEvent { Request(bool), RegistrationDefault(true: `!`, `#`
with a reference actually passed), NameUse(body flag), RunTime(true: SUMIF shape, spill block) }`; decide with one
function: `flag = S; for e in events { flag = e.value }`. Replace `state.volatile`/the OR at
`recalc.rs:2613-2619` and `5343-5346`; the bit belongs to the last completed evaluation, replaced at publication,
carried when a result is reused, never taken from an abandoned or waiting attempt. A cell loaded from a file starts
from its `ca` bit (X6). The registration default needs the arguments' reference-ness at call time. A request
inside a lifted multi-cell call is dropped (X2). The user-function host must answer `xlfVolatile` with status 2
outside `#` registrations and status 0 inside them and outside cells (DNA answers it everywhere).
Names: volatility is a fact of the body, joined at each executed use, plus the dirtying on redefinition; do not fold
name bodies into S.
Decided, closing the open items of the plan: F2 holds (F1, F3 dead); S is syntactic over the whole formula, not
through names; the `!` default is an event at call start and clearable only by a request of false that Excel
accepts; `#` volatility needs a reference passed; a name use is an event.

## 8. Not measured, still open

- WEBSERVICE and other service functions; CUBE and RTD functions.
- Volatility of spill-blocked cells against later requests (only SUMIF and names were ordered against requests);
  dependents of a spill; formulas with array-entered (CSE) storage; whole-column forms of the SUMIF rule.
- `$` plus `#` together; XLL `>X` functions registered `#`; `$` functions called by the thread pool (single
  calls only); several Excel threads racing (MTC was on, a function with requests ran on the main thread in
  every trial).
- Automatic-mode calculation on open and Open-event behaviour; `CalculateFull` in Automatic mode; workbooks with
  `fullCalcOnLoad`; iteration mode; data tables.
- The reference-only dependents quirk (section 1): its cause and its rule for ranges and names.
- Why Excel died between trials in some multi-trial runs (`RPC failed`, three times, no pattern found); each
  affected reading was repeated in its own run (`x4r2`, `x4r3`, `p5r2`, `p5r3`).
- Behaviour on other builds, on Mac, and with Excel's calculation version other than 191029.
