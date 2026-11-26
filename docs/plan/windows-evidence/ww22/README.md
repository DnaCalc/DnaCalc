# ww22 Excel evidence

The bead comment text in [bead-comments.tsv](bead-comments.tsv) goes to the tracker through the coordinator. Each capture includes the probe, Excel build and CPU, raw observations, and the private desktop guard when available.

- `sheets/` and `vba/` cover cross-workbook copy and move, grouped move, range activation, and VBA object members. Three first-pass group calls used a PowerShell COM binding that rejected the array before Excel saw it. The VBA rerun has the Excel answers. The first calculation batch stopped when its cleanup tried to set Calculate with no workbook open. Use `calculate/` for the replacement run, which includes the SheetCalculate observer and canceled save-on-close.
- `paste/` and `followup/` cover paste widths, notes, conditional formatting, blank arithmetic, CutCopyMode, AdvancedFilter, and format variants. Use the VBA CutCopyMode results. The first PowerShell COM binding rejected inputs other than Boolean. Use `BlankOnBlank` from followup for blank destinations.
- `protect/` and `allow/` cover protected commands, password coercion, and persisted sheetProtection. The Allow flags were applied individually. The PivotTable control only measured the flag readback.
- `projects/` covers synthetic Excel-created projects. `authored/` replays the Linux lane's own workbooks where available. Keep the package inventory beside the live reads when interpreting sheet code names and project streams.
- `charts/` covers point and label commands, reopened chart frame edits, and raw drawing/chart XML collected from their saved packages.
- `bank-rows/` records differences against the behavior or typed refusals in master 8dd3c1a67 and the lane's linked bead evidence. No product build ran in this round.

To replay, copy the probe and helper into the same layout under an owned scratch folder. The included run script submits the selected probe through the DnaCalc-win private Excel session. The published guard and provenance describe the original runs.
