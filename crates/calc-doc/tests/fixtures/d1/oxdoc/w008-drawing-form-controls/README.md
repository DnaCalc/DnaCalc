W008 drawing/form-control corpus fixture.

This fixture intentionally mixes modeled and deferred drawing-control surfaces:

- classic and x14 form-control property parts
- button, checkbox, option button, group box, list box, dropdown, spinner,
  scrollbar, label, and a future/unknown control type
- raw macro assignment, linked-cell/range formulas, and scalar control values
- multiple controls sharing one DrawingML part
- picture, chart, and OLE-like drawing payload blockers in that same drawing part
- legacy VML drawing data with likely form-control client data
- a control whose shape id is missing from the drawing part
- a control whose shape id resolves to duplicate drawing objects
- unknown x14 extension and unknown property child preservation notices

Charts, OLE/ActiveX behavior, VML editing, rendering, and control behavior are
not supported by OxDoc here. They are present only as preservation/blocker
evidence for the W008 file-format boundary.
