# P06 result

Pending-async supplement: [P07/save-ui](../P07/README.md) records ten UI Manual Save(false) trials. Ctrl+S
canceled the calculation; live Error2051 remained while the file preserved the last successful numeric caches
41/42 with edited input 201..210. This is not evidence of saving a still-live pending continuation.

Manual Save(false) preserves stale formula caches: choose `PreservePermittedCaches` for that profile.
The later first-workbook supplement confirms the ordinary Manual/Automatic distinction in fresh processes;
the total retained package count is now 1551 (1528 in the original save/reopen runs, one synthetic mode variant,
two startup controls and twenty repeated startup controllers).
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

First-workbook supplement: `run-first-workbooks.ps1` uses the same supervisor with FirstWorkbook. Its process
argv opens the measured source, before any controller. After exact PID/start/HWND ownership and guardian setup,
COM records A1:C1, mode and CalculationState before any diagnostic macro. The source's ReadAfterOpen records
typed values and counters; the source closes without saving. Only then does the worker open the original
controlled macro-free bootstrap, whose five-part/hash/compile checks remain unchanged.

`first-open-summary.json` and first-manual/auto-NN-trace.tsv retain twenty accepted fresh-process observations.
All ten Manual starts selected mode -4135, state 2, A1/B1/C1=5/2/3 and no PlainProbe entries. All ten Automatic
starts selected mode -4105, state 0, values 5/10/11 and exactly one call each to IDs 32/33; B3/C3=5. Every source
hash before/after matched. The Manual source is a genuine current-version Save(false) file; Automatic uses
synthetic-first-open-auto.xlsm, made by changing only its calcMode to auto. The source/change hashes are in
first-open-synthetic-manifest.json. This controls startup mode while holding formulas/caches fixed; it is not
represented as an Excel-authored stale Automatic save.

The Manual/Automatic startup controls were each tried once before the repeated runs. Automatic repetition 5
initially failed before durable ownership because a transient startup window could not be captured by UIA;
there was no measurement from that attempt. Resume retained the failed raw directory and ran a fresh retry ID.
The successful twenty runs have compile/run ok and worker cleanup owned-process-zero. The parent sealed
expected-value mismatch and occasional exited-process identity race are retained, not represented as behavioral
test failures or as proof of a live leftover process. The original source hashes and the worker cleanup records
are the measurement authorities. first-owned-process-audit.json also confirms all twenty accepted process IDs
were no longer running. first-saved-caches.json covers all 1551 packages; supervisor-raw.zip includes
the original runs plus startup evidence. Disabled-sheet first-open and genuine older-version packages remain
outside this ordinary startup control. Pending-async saves are recorded by P07's separate latch experiment.
