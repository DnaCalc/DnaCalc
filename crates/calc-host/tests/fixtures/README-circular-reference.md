# Circular Reference

This workbook was authored in this repository on 2026-11-12 from the game
concept in `docs/fun/circular-reference.html`. Its formulas and package are
original work, contain no external workbook content, and are covered by the
workspace MIT licence. Generate it with the existing oracle package writer:

```powershell
./crates/calc-func/tests/fixtures/make-catalogue-fixtures.ps1 `
  -OutDir crates/calc-host/tests/fixtures -CircularReferenceGame
```

Open `circular-reference.xlsx` with iterative calculation enabled. Each
recalculation is a tick: type `L`, `R`, `U`, or `D` in B2 and press Enter. Food
is shown as a number; #DIV/0! ends the game, as do the edge and the snake's
own body. B5 is the calculated SUM, D5 shows its formula, and F6 expresses the
binary64 SUM exactly as a rational number. B6 is the snake length.

To reset, enter 1 in B3, clear B2, then enter 0 in B3. The first three food
positions contain 0.1, 0.2 and PI(); each later food position is chosen from
the current tick when food is eaten and stays in place until eaten. The hidden
U1 holds the self-referencing game state.
The initial state is an authored workbook cache; it is not a calculated result.
The host test requires calculated provenance after each move.

`circular-reference-moves.txt` supplies three games shared by the host test and
Excel session job. The 46 moves end in #DIV/0!, circular reference and #REF!,
in that order. The test asserts all 192 board cells and the status after each
move. Excel comparison is a separate measurement and must be run in Excel; the
fixture's existence and the host test alone do not establish Excel parity.
The current host calculates all 198 formulas in the scripted ticks but marks
their iterative results uncertified, so the app reports an incomplete
calculation. The remaining certification work is tracked by `dc-hsa5.14`.
