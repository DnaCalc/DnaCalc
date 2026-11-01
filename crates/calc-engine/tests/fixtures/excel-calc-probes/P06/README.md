# P06 result

Manual Save(false) preserves stale formula caches: choose `PreservePermittedCaches` for that profile.
Excel 16.0.20527.20088 x64, CalculationVersion 191029, Windows/AMD Ryzen 7 4800H, safe mode.
440 primary save cases, 30 scope controls and 320 after-controller opens; 1,528 retained packages,
including eight explicitly synthetic flag variants. No recorded probe faults; owned-process-zero cleanup.

For ordinary Manual Save, SaveAs and SaveCopyAs, false kept A1=5 with B1/C1 caches 2/3, dirty formulas
with ca=1, calcCompleted=0 and calcOnSave=0. True updated B1/C1 to 10/11. Save(true) also recalculated
a dirty peer workbook (B1=10 and UDF C1=5 versus stale 2/1), visibly changing live caches outside the
saved workbook. Do not normalize this command to dirty demand on the saved workbook alone.

BeforeSave edits reached Save/SaveAs files: A1=7, with false caches 2/3 and true caches 14/15.
SaveCopyAs did not raise those handlers and kept A1=5. Use **-tested.xlsm** for the tested save image;
the later base artifact SaveAs can run the still-armed handler and therefore differ from that image.
An Excel-compatible serialization pin must include the admitted BeforeSave edit.

With Disabled active and EnableCalculation=False, Save(true) left both S and Disabled stale in ten
trials. Activating S first let Save(true) update S while Disabled stayed stale in ten controls.
The latter disabled-sheet caches have plain formulas without ca; calcCompleted can be absent in
stale-cache files. Neither flag's absence proves freshness.

The corrected TABLE control changes an independent multiplier Z1=1 to 10. In Automatic Except Tables,
H2/H3 were still 2/3 immediately before Save, then became 20/30 during Save with **either** value of
CalculateBeforeSave (ten each). Save(false) is therefore not universally zero calculation. The first
table setup changed only the table input, so its unchanged table results were not diagnostic;
those original trials are retained alongside the independent-input control.

After a controller workbook, 60 ordinary tested files opened in each mode: Manual retained 2/3 from
false and 10/11 from true; Automatic showed 10/11. Ordinary Manual opens had no UDF entries. Eight
synthetic variants (one unchanged control and seven perturbations of old/future calcId,
fullCalcOnLoad, calcCompleted, ca and sheetCalcPr)
each had ten opens per mode: Manual retained 2/3, Automatic showed 10/11. These do not establish
flag combinations or behavior of genuine older-version packages. Disabled-sheet opens restored
EnableCalculation=True; notably the inactive-disabled file's plain-formula stale cache stayed 2/3
even in Automatic, while S stayed 10/11. Imported caches remain unverified values.

Raw groups: raw-results.tsv (primary), scope-raw-results.tsv, reopen-raw-results.tsv,
flags-raw-results.tsv, disabled-raw-results.tsv. Their manifests retain exact XML and hashes before
reopen; the archive retains all injected sources and rejected or earlier trials. Synthetic changes
are recorded by make-synthetic.py and synthetic-manifest.json; formula/cache values were not edited.

Not yet measured: opening the tested file as the first workbook. The reusable supervisor opens a
controlled macro-free bootstrap first; a startup-source extension with the same ownership and hash
checks is needed for that condition. Pending-async saves belong to P07's separate latch experiment.
