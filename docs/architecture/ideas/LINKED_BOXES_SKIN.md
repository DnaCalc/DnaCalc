# Idea: a linked-boxes skin over defined names (for discussion)

Owner idea, 2026-11-11. Status: queued for discussion, not scheduled. Nothing here is decided.

## The idea

A free-form canvas skin for DNA Calc. Instead of a grid, the user sees **boxes** wired together. A box is a named
calculation unit with **inputs** and **outputs**. Nothing new is stored in the file: every box, input and output is
a **defined name**, so the workbook stays an ordinary Excel workbook that Excel can open and calculate.

- Box `MyCell` with input `Input1` and output `Result` maps to the defined names `MyCell.Input1` and `MyCell.Result`.
  The canvas shows `Input1` and `Result` on the `MyCell` box; showing the box prefix is optional.
- **Wiring** `OtherCell.Output` to `MyCell.Input1` sets the definition of the name `MyCell.Input1` to
  `=OtherCell.Output`. Unwiring sets it back to a literal or leaves it empty.
- **An output** is a name whose definition is a formula over the box's inputs, such as
  `MyCell.Result = MyCell.Input1 * 2`. Shown inside the box as `= Input1 * 2`, with the prefix stripped.
- **Layout**, meaning box positions, sizes and colours, is presentation only. It lives in a skin part of the file
  (custom XML), never in the calculation model, so Excel ignores it safely.

## Why it fits what we have

- Defined names are already first-class in calc-doc and the engine, including scopes, `LET`/`LAMBDA` bodies and
  name calculation (the dc-hsa5 work).
- Excel allows dots in names (`MyCell.Input1` is a legal name), so the mapping needs no escaping in the common case.
- It is a skin, not a new engine: the same recalculation and the same Excel parity apply.

## The formula-name presenter and interpreter (a skin IR piece)

The canvas, and the notebook skin, both need to show formulas **in the vocabulary of the skin** rather than raw
workbook text. Proposed: a small layer in the skin IR with two directions.
- **Present:** take a formula and a context (which box is being shown) and rewrite name references for display:
  `MyCell.Input1 * 2` becomes `Input1 * 2` inside `MyCell`, and `OtherCell.Output` stays qualified.
- **Interpret:** take what the user typed in that context and resolve short names back to full defined names, with
  the same rules, before the formula reaches the engine.

It works on the parsed formula (calc-fml's syntax tree), never on text, so ranges, strings and functions with
similar names are not touched. The notebook skin can reuse the same layer: a notebook cell named `Revenue`
referring to `Costs` and `Price` is the same idea without the canvas.

## Questions for the discussion

1. Box scope: workbook-scoped names only, or also sheet-scoped (`Sheet1!MyCell.Input1`)?
2. Arrays and spills: may an input or output hold a whole array, shown as a small table on the box?
3. Cycles: the canvas makes cycles easy to draw. Refuse the wire, or allow iterative calculation as Excel does?
4. Grid interplay: may a box output be placed into a grid cell (`=MyCell.Result`), and a grid range be an input?
5. Excel round trip: workbooks saved from the canvas must open unchanged in Excel. Do we also need a reverse
   import, which builds boxes from an existing workbook's dotted names?
6. Where it sits: a DNA Calc skin only, or also in OneCalc as a playground for a few named formulas?
