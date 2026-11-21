# Formula editor and large-array results

These legacy OneCalc design notes apply to the browser formula and result surfaces. They
capture interaction rules that can be evaluated independently of the old host and bridge.
The shared formula UI lives in `apps/dna-formula-ui`.

## Formula editor

- Give the native textarea ownership of the caret, selection, IME composition, clipboard,
  browser undo and accessibility. Arrow keys and click-to-position stay in the native input path.
- Draw syntax and diagnostics in presentation layers aligned to the input's font, padding
  and scroll position. Noninteractive overlays leave pointer events to the input. Measure
  popup placement from the browser's layout and recompute it on scroll, resize and zoom.
- Coalesce automatic analysis after edits. Associate each result with the text and caret
  revision it describes so a late response cannot replace diagnostics or suggestions for
  a newer edit. Keep direct typing responsive during analysis and composition.
- Keep completion and signature help anchored to the active token and argument. Expose help
  and diagnostics to assistive technology and honor reduced motion. A local fallback can
  keep the input usable and disclose its source when analysis is unavailable.

Sources from DnaTreeCalc `9644443`. [Formula editor UX specification](https://github.com/DnaCalc/DnaTreeCalc/blob/9644443788b1596939ea2f04a546c4fcaa954d1b/docs/onecalc/APP_UX_FORMULA_EDITOR_SPEC.md), [WS14 editor design](https://github.com/DnaCalc/DnaTreeCalc/blob/9644443788b1596939ea2f04a546c4fcaa954d1b/docs/onecalc/WS14_DESIGN_FORMULA_EDITOR.md).

## Large-array results

- Report total rows and columns alongside the bounded preview. If part of the result is hidden,
  display how many rows and columns were omitted. The source design uses a six-by-six first
  preview and a windowed, virtualized view for larger results.
- Use typed cells so numbers, strings, blanks and errors keep their format and identity across
  previews, inspection and comparison. Reuse the cell renderer across those contexts.
- Load visible windows on demand and keep the first render bounded by the preview. In a
  side-by-side comparison, synchronize scroll position so corresponding cells stay aligned.
- Label copying the visible window and copying the entire array as separate actions. Fetching
  the full array for inspection or copying follows an explicit user action.

Source from DnaTreeCalc `9644443`. [WS14 large-array design](https://github.com/DnaCalc/DnaTreeCalc/blob/9644443788b1596939ea2f04a546c4fcaa954d1b/docs/onecalc/WS14_DESIGN_LARGE_ARRAY_RESULTS.md).
