# Windows and Excel queue

Tracker sweep dc-r8l0g on 2026-11-22 reviewed 162 open beads with a `visited:` note at master
`52b93962` and checked 107 copied handoffs. The rows below are pending or have a named prerequisite.
Run shared probes once and record the results against each bead that uses them.

`F/<lane>` is the `~/codex-runs/<lane>/RUN-ON-WINDOWS.txt` on dna-firehorse. `K/<lane>` is
the same path on dna-koderbot. The coordinator's copies were under `tk2/windows-lists/`.
`tk2` entries are also in this sweep's `~/codex-runs/tk2/RUN-ON-WINDOWS.txt`.
Original lists supply fixture paths and command arguments where a row refers to them.
Use owned sessions and authored fixtures for live Excel. Compare retained corpus captures offline.
Record the Excel build, CPU, exact input words and result selection for function probes.
Keep row-level Excel discrepancies as WitnessSet JSON and keep frozen predictions separate.

## Workbook calculation, VBA, COM and add-ins

| Bead | Lane handoff | Pending step |
| --- | --- | --- |
| dc-0f1k.10 | K/om1 | Use the interactive Formula Explorer Load add-in picker on the authored X1 fixture. Check its listed name and SHA-256, evaluate a known X1 row, unload it, then reopen and cancel the picker. The milestone page run already has 350/350 comparisons. |
| dc-0f1k.4 | K/om1 | In a fresh authored XLSM compare a normal Workbook_BeforeSave return, Cancel=True and an unhandled Err.Raise. Record file creation or changes and Excel's error. The milestone page run already has 263/263 comparisons. |
| dc-1nbp.3.1 | K/px1 | Run the current `appevents-page-plan.json` with the original X0/X4 stage and `--allow-unmatched`. Take matching Excel one-load and two-load controls and enter the final formula through both UIs. Retain Target and the ordered Change, SheetCalculate, AfterCalculate and PyTargets records. |
| dc-1nbp.6 | K/px1 and tk2 | Run the remaining answer 14 E2/E7 and E3 B/E4 remainder/E5 remainder arms from `14-harvest.md` using the P22 logger where applicable. Reuse the three retained trials for E1/E6/E11. Share the E4 UI-entry run with dc-1nbp.3.1. |
| dc-1nbp.7 | tk2 | Author a Worksheet_Change handler that reads Me.Range and another sheet. In Manual and Automatic, edit the same cell through the native UI and replay through the DNA Calc page. Retain ordered events and values, with descriptor binding owned by dc-piw0.5. |
| dc-1nbp.11 | K/px4 | Run the 120 authored workbook trials in answer 15 section 4 (`15-harvest.md`), X1, X1b, X2, X3, X4, X5 and X6, using the existing calculation probes. Record raw bits, ordered Tk/Cnt entries and commands, and saved `ca` bits. Route X3 to dc-1nbp.9, X2 to dc-hsa5.6, X5 to dc-hsa5.14 and the trace work to dc-kapl.2.1. |
| dc-1nbp.14 | tk2 | Run answer 16 E13 with calcOnSave absent, 1 and 0 as the first book in a fresh process, plus the two contrasting opening-order arms. Read Application.CalculateBeforeSave, edit a precedent, save and record calcPr and caches. Capture owner dc-1nbp.19. |
| dc-1nbp.15 | tk2 | Run all four answer 16 E12 arms, Manual and Automatic, with 1,000 formulas including ten counting-UDF formulas. Insert row 5 or rename a referenced table column, then Ctrl+Z. Record displayed values and calls before and after F9. Capture owner dc-1nbp.19. |
| dc-1nbp.17 | tk2 | Run answer 16 E6 on an authored workbook with chart series that reference a removed sheet. Save the unedited, renamed and removed-sheet copies and compare chart c:f, cached points, tables and rule references. |
| dc-1nbp.23 | K/px4 | Run answer 17 E3 dotted-name binding and legality on two sheets, then rename and delete/add with save/reopen. Run E7 named-cycle keys and array-name Evaluate, E2 relative-name Evaluate across two windows, and optional E1 Name Manager UDF logging. Use a separate research workbook for value probes. Reuse the retained P12 captures for their existing scope and send presenter results to dc-lvlj. |
| dc-1nbp.27 | K/px4 | Run answer 18 E1-rest, E3-macro, E5-MTR and E6. Reuse x17 `QU$`/`QU`/`QU#` and the P-series, and use an authored LongProbe for the native interrupt arms. Retain thread IDs, raw status 64 and re-entry, before/after macro reads, sweep bits, and Esc outcomes through the next calculation and save/reopen. Feed dc-1nbp.28 and dc-hsa5.20.5. |
| dc-2nn1 | F/oc2 | Run W2 on a controlled Windows mapped share and its UNC path. Record the origin and open decision in both desktop apps with the same file, plus the local control. |
| dc-6332.10 | K/px1 and F/ah1 W1–W2 | Run native calc-xll-abi and calc-xll-host tests. Re-verify archived X1 using its original Excel session ledger and run paired traced/untraced X2/X3/X4, ribbon, spill-pool, workbook-pool and X5 RTD captures. Preserve the four C.1 counts and strict sequence verdict. |
| dc-6332.11 | tk2 | Finish D.1 framework and MTR-setting repeats and D.7 async cases from answer 13 in owned Excel sessions. Preserve raw return status, callback and continuation order, and one manifest per framework/MTR run. |
| dc-6332.12 | tk2 | Run answer 13 step 3/4 lifecycle and edit-aware command-entry controls through Excel and the desktop host. Capture ConnectData, command and macro entry, publication, DisconnectData and close order. COM apartments belong to dc-6332.16 and duplicate initial RTD publication to dc-snz.23.3. |
| dc-6332.16 | F/ah1 W5 | Reuse the repeated Excel COM_IDENTITY and COM_STA2 control records in 2b75c436. Run the same x19-com-identity cases through the Formula Explorer host, including the D.15 no-Office Windows machine, and retain exact result strings or the owned process's failure log. |
| dc-7jz5 | tk2 | Run the original T1–T3 acceptance checks on native Windows against 2141cb13, including mapped-drive and UNC origin controls. Record the actual decisions. Share the mapped-drive setup with dc-2nn1. |
| dc-hgie | tk2 | Run the P1–P7 original native acceptance scenarios on a controlled Windows host using authored fixtures. Capture COM dispatch and native-path decisions against the landed review fixes in 2141cb13. |
| dc-hsa5.4 | F/en1 | Use the P07 native-provider controller. Run Manual F9/Ctrl+S with CalculateBeforeSave false and true and an edit held during serialization or release. Capture live values, disk caches, calcPr and ordered entry, return, cancellation and save events. Extend the P01–P08 fixture families as specified in en1. |
| dc-hsa5.6 | F/en1 | Run all six answer 15 X2 name arms three times with the P12 route. Capture shadow edits and missing-name repair, two relative anchors, and one versus two stateful uses. Capture owner dc-1nbp.11. |
| dc-hsa5.12 | K/eb8 and K/eb7 | Use the retained P22 source jobs to capture native UI and provider-wait ordering and the handler failure after commit. Keep BeforeSave SaveAsUI, cancellation and failures distinct. |
| dc-hsa5.14 | F/en1 | Run X5's four arms three times and record seeds, bits, Cnt entries and retargets with iteration on and off. Add separate authored array/spill, MTC chain-history, and scoped/linked circular Range controls. X5 capture owner dc-1nbp.11. |
| dc-hsa5.23 | F/en1 | Run the ParamArray owning-session test and the simple workbook integration replay on Windows. Compare the nested DnaArgs Worksheet_Change in an authored Excel workbook, preserving event order and Value2 subtype/text. Keep DnaSum For Each as an upstream compiler control. |
| dc-piw0.4 | F/eng3 item 4 and tk2 | Use the retained 39-line Excel answer and the authored BuildReport fixture. Run the complete desktop VBA replay when dc-piw0.5 supplies the shared descriptor binding. Check the listing and repair-free desktop save/reopen. |
| dc-snz.2 | F/ah1 and tk2 | Carry the existing 289/289 X1 and 350/350 milestone evidence forward. Exercise the Windows page command/help scenarios and pinned Excel-DNA/C comparisons. RTD sequence parity is owned by dc-snz.23. |
| dc-snz.6 | tk2, using the ad1 B17 finding | Use the retained B17 fixture and build-20527 result as the control. Compare clean Calculate for a bare-reference UDF dependent versus an expression argument and an authored dirty edit. Record call counts per caller in VM3/JIT and preserve the existing registration and unload controls. |
| dc-snz.9 | K/eb6 | After provider identity wiring, open the authored linked workbook and XLAM controls through the Windows application. Check actual resolved provider, precedence, and rebuild on unload. Include distinct providers sharing a basename. |
| dc-snz.21 | K/om1 and F/eng17 | On an owned Windows desktop use the existing xlOil Python m4 reload case to capture load, teardown and reload of the same add-in, including the refusal/diagnostic. Keep the verified native Utils/SQL replay as a control. |
| dc-snz.23 | F/en1 and K/px1 | Replay current native X5 including concurrent calls, async completion and RTD, with the same Excel case and host configuration. Keep the ordered provider events and strict client-call verdict. The initial duplicate was resolved in 4acacf0d. Retain the 1,42 control while checking managed lifecycle, callback permissions and cancellation. |
| dc-snz.19 | tk2, reconstructing ad1 from F/eng17 and the tracked X2 recipe | After the upstream loader/runtime fix, use the pinned X2 Native AOT 1.10.0-preview5 binary in separate owned Excel and Formula Explorer processes. Run load, call, unload and same-binary reload, then try a fresh host and an alias. Record second initialization and the typed refusal or abort in the appropriate control. |
| dc-y2o.1 | F/eng12 item 3 and tk2 | Run the Q19/Q23 circular-reported-cell controls against candidate rules. Use Q23.RunAll for its six scripts and retain the circular log, workbook snapshots and each calcChain. Compare retained Q23 evidence in 180ff6ed before requesting duplicate observations. |
| dc-yqq0.28 | tk2 | On Windows run the CalcVBA live COM slot-thunk/record-array suite from the vx7 change in both VM3 and JIT. Record early/late-bound, ByRef/retval and IRecordInfo cases and retain architecture-specific results. Cross-target type checks give separate evidence. |

## Workbook formats, grid, drawings and printing

| Bead | Lane handoff | Pending step |
| --- | --- | --- |
| dc-1nbp.22 | K/px4 | Run answer 17 E8 on authored `cdoc_m2_1` fixtures. For each of the three packages, save unchanged, after a value edit and after inserting a row above a block. Add XLSM and XLSB SaveAs for one package. Retain custom XML, relationships and hidden locator `RefersTo`, then reload all 11 outputs in DNA Calc and record anchors and `LayoutWarning`. |
| dc-4j0e.2 | K/eb9, referring to K/cx2 | Use the authored cx3 inputs and the 67 normal, 21 repair and 20 formula case instructions. Score raw RK/MULRK bits, continued SST encodings, empty/trailing phonetics, row guides, BLANK/MULBLANK and row-height flags. |
| dc-4j0e.3 | K/eb10 | Use authored compound token fixtures to measure remaining fixed-arity and legacy-name conversions plus shared, table and array Formula/Formula2 readback. Preserve converted package records and plain-open versus repair outcomes. |
| dc-4j0e.4 | K/eb10 | Measure the authored key-4 Macintosh FileMoniker and parent-relative link controls. Read Hyperlinks Address/SubAddress and inspect saved relationship targets and repair outcomes. |
| dc-4j0e.8 | K/eb10 | Run the owned BIFF5/7 Book-codepage and CONTINUE controls. Capture Value2 and saved XML for continued text and formula caches, including macOS code pages and unusual BIFF5 layout cases in the handoff. |
| dc-4j0e.14 | K/eb9, referring to K/cx2 | Use authored raw BIFF2/3/4 fixtures. Verify Excel SaveAs 16/29/33 signatures and measure tSheet/tEndSheet names, legacy formulas/arrays, borders and formats. Keep the three token-0x1A failures visible. |
| dc-8wp.6 | F/pv1 | Extend the M1 probe with Region/Product crossed with Month, tabular/outline, three Values positions, Sum/Count/Average and subtotal/source errors. Save TableRange2, axis items, package and raw numeric bits. |
| dc-8wp.10 | F/pv1 | Capture Prec inputs before refresh and after reopen, particularly G6 near 1e15, cache spelling and Sum/Average/variance. Vary row, column and source order independently for grouped Product, including overflow and empty numeric groups. |
| dc-8wp.12 | F/pv1 | Measure Dark6/Dark11 borders, neutral/accent variants and conditional-format precedence. For AutoFit retain sheet font, DPI, Range.Text and widths before and after, including clipped numeric cells. |
| dc-8wp.15 | F/pv1 | After layout transport and field list land, set DNACALC_PIVOT_SAVES to an empty lane directory, run the cited dc_8wp_15 save test, then use the existing pivot open replay for the resulting desktop saves. Compare M5 cells, pivot structures and repair logs. |
| dc-8wp.16 | F/pv1 | Extend M5/M7 with two pivots sharing a cache and a source-column deletion leaving each with different surviving fields. Capture structures, cells and reopen. Use separate owned attempts for Location overlap and partial shifts. |
| dc-8wp.19 | F/pv1 | Save model packages before and after the Sales–Regions relationship. Capture endpoints and keys, unmatched-key and MDX results before/after RefreshAll, and DAX raw bits across magnitudes and types. |
| dc-971p.16 | K/gl1 | Measure RowHeight/ColumnWidth after AutoFit for plain and wrapped text, wrapping with rotation, shrink, fill, justify, distributed and merged cases. Pair the offset-pane edit/freeze probes with dc-971p.62. |
| dc-971p.43 | F/eng18 item 3 | Run `cargo xtask app paint-compare calc --out <dir> --zooms 75,100,150` on generated Calibri 11 inputs. Compare text_ink_box and baseline counts to the styles-less control with Excel's default font. |
| dc-971p.43.1 | K/eb11 | Run paint compare at 100% on matching authored paint-everyday. Record row 10 and subsequent DOM/Canvas geometry plus height XML for omitted, noncustom and explicit height. Include hidden and merged rows. |
| dc-971p.43.2 | K/eb11 | Run paint compare for matching paint-panes and paint-everyday inputs at 75/100/150%. Score active-cell exception, edges, fill class and fill handle in DOM/Canvas against the matching Excel input hashes. |
| dc-971p.43.3 | K/eb11 | At 96 DPI measure header extents at 50/75/100/125/150/175/200%. Scroll across 10, 100 and 1000 digit thresholds with frozen and non-Calibri controls. Keep viewport, digit widths and partial rows. |
| dc-971p.43.4 | K/eb11 | Capture same-font text at 50/75/100/175/190/200% and compare ink, baseline and width. At 50% bracket the numeric overflow hash by 1 px. Measure number-as-text markers for apostrophe, Text, General, formula and ignored-error controls. |
| dc-971p.57 | K/gl1 item 4 | In newly authored books compare Excel preview and printer output for an iteration-off circular hold and a closed linked source with updates declined. Pair printer-dependent measurements with dc-971p.62. |
| dc-971p.62 | K/ux6 | In fresh workbooks compare native IME and proportional-text double-click caret positions, selection aggregate visibility, scrolled freeze and wheel zoom, spill outlines across panes, and formatted or merged clipboard transfer. |
| dc-971p.64 | K/eb11 and K/eb6 | Run the exact authored sort matrix for unequal collapsed groups, merges/covered keys, full/partial CSE/spill, range/table filters and another-sheet relative keys. Save rows, hidden criteria, metadata, Formula2 and exact refusals. |
| dc-971p.65 | K/gl1 | Inject partial manual breaks into authored A1:P40 at row 20 and column 8, with and without pt=1. Compare full-span and disjoint-area controls, PDF page rectangles/order, page-break COM fields and XML after Save As. |
| dc-971p.73 | K/ux5 | Compare consent and retained contents for Merge Cells and Merge & Center over populated and overlapping ranges, and confirm how Excel handles partial Unmerge selections. |
| dc-cazfj.1 | K/eb9 | Create controlled XLSB/XLSX twins for chart sheets, print settings, pivot caches/tables, query tables, slicers and extensions. Measure repair using synthetic malformed layouts. Keep library files closed in Office. |
| dc-hjd | K/eb1 | Read Formula and Formula2 after native assignment of the five spaced-colon expressions in the authored A1:A3 control. Compare stored text and native grid admission to the recorded socket answers. Entry work is owned by dc-mx3x. |
| dc-mlhgn | K/eb6 | Recover original WRAPCOLS placements, capture the addressed reference-info and large spill cells with the existing owned route, and record placements, blockers and exact array shapes. Pivot-bank work belongs to dc-8wp. |
| dc-mx3x.1 | K/eb6 | On authored malformed XLSX copies run normal open and CorruptLoad for shared/legacy array members with full, mixed or missing caches and nonnumeric singleton caches. Record repair lines, surviving formulas, caches and readback. |
| dc-snz.10 | K/eb6 and K/cf1 | Use authored XLSB controls for flags-0 literal and flags-0x0e VBA calls. Record BrtName and saved tokens, exact formula and value after open and edits, with binary output owned by dc-snz.14. |
| dc-snz.14 | K/cf1 | When the binary writer is available, compare authored XLSB edits and reopen through Excel. Inspect sheet/workbook records, BrtName/BrtSupName, shared and array tokens, untouched opaque parts, and repair logs. |
| dc-tphe | K/eb1 | Regenerate the circular GAME fixture into fresh output. Submit the owned iteration job by the new hash, compare all 46 GAME records, and run Firefox/Excel paint-compare at 100%. Preserve the original capture on its original hash. |
| dc-ylr | K/eb9 | Run `make-dc-ylr-oracle.ps1 -SourceOnly` through the owned-session route in 78b95568. Compare oracle.xlsx with and without its Excel-authored source, rerun offline oracle corpus, retain existing captures and commit ylr-source.xlsx. |
| dc-zjsg.4 | F/dw4 item 2, filed there under dc-zjsg.9 | Run plain open and forced-repair controls on regenerated authored kept-objects.xlsx with the minimal Package1.bin. Record any dialog or removed/repaired record. OLE fidelity ownership is dc-zjsg.9. |
| dc-zjsg.5 | F/dw4 item 1 | Measure all three curved-connector detours after moving the owning shapes. Record XML preset, rotation, flips, extents and avLst adjustments. Treat the measured bent-connector result as the control. |
| dc-zjsg.6 | F/dw4 item 3 | Run plain and forced-repair opens of regenerated authored chart-kinds.xlsx and paint-compare Treemap, Sunburst, Histogram and Pareto. Preserve per-kind geometry and repair results. |
| dc-zjsg.8 | F/dw2 and tk2 | Use the authored form-controls sheet to recheck Drop Down 8 TextFrame.Characters().Text and linked cells. Add inert ActiveX coverage on the authored fixture. Six working linked controls are already recorded. Button macros belong to dc-euei. |
| dc-zjsg.10 | F/eng18 and F/dw4 | Finish matched Windows paint-compare for shape/picture fixtures and the sparkline thin-weight controls at 100/150/200%. Keep the shape parity fields and ink widths against the retained measurements. |
| dc-zjsg.13 | F/dw5 | Run the authored chart value-axis scale recipe with plot-height variants, preserving used plot dimensions and numeric axis min, max and major units. Compare the same chart types and source vectors across heights. |

## Excel function and CalcVBA measurements

| Bead | Lane handoff | Pending step |
| --- | --- | --- |
| dc-078b | K/image3 and F/eng10 item 2 | On the owned Excel worker compare a verified rendered HTTPS picture to refused HTTP and unreachable HTTPS. Record Value2 and the settled rich/picture observation together so two #VALUE! results can be interpreted. |
| dc-0f1k.8 | F/wb2 and F/onecalc1 | Run the localized formats job in matching installed Excel locales for the 61 pending rows. Record actual locale, formatted output and separators, against the same typed values and masks. |
| dc-3k5n | tk2 | Use newly authored examples of the retained locale-specific operator and function coercions on corresponding Excel installations, observing separators and typed values. The original Enron comparison needs retained captures and stays owned by dc-3k5n. |
| dc-4oy5 | K/eb6 | Finish original infix coercion via authored controls. Capture Formula versus Formula2 and grid entry, including raw typed results for the mined input classes. Carry the established 15/15 TIMEVALUE and 81/81 per REPT book forward. |
| dc-6332.7 | K/px1 | Continue the applicable B.1 prefactor probes from answer 9 where ax1 and nx6/nx7 still diverge. Freeze predictions, preserve exact inputs and observations, and record B.2–B.10 progress against the owning experiment plan. |
| dc-6332.8 | tk2 | Run outstanding E1–E11 discriminators from answer 10 using the retained recurrence, asymptotic and wrapper-coordinate inputs. Reuse observations in b4131135/6d0ac12d and capture only remaining discriminants with exact arguments and bits. |
| dc-b1l | F/fn4, F/fn8 and F/fx1 | Use the authored lifting, AGGREGATE, typed-entry and 3-D controls and capture the pending sections using the existing oracle path. Pending capture owner dc-b1l.6. |
| dc-iuwct.51 | F/ig2 | When the BETA.INV crash budget permits, resume single-step large-shape/LowerEndpoint probes using both 6/7 alias inputs and the exact dyadic b=1 discriminator. Retain exact words through the damping and store sequence. |
| dc-iuwct.52 | K/ig3 | Run BINOMDIST.json with both exact rows plus BINOM.DIST and BETA.DIST companion files. Compare substitution of each observed prefactor in the retained fraction. |
| dc-iuwct.53 | K/ig3 | Run the 77 F.DIST PDF rows with staged density companions. Bracket d2=1048576 and 1e10 on the structural set, vary adjacent x and include separately rounded beta coordinates and LN/EXP staging. |
| dc-iuwct.62 | K/ig3 | Run LINEST.json, selected outputs and covariance controls by result_index. Compare covariance diagonal/translation cross terms over the six separations and retain exact residual-sum and df controls. |
| dc-iuwct.63 | K/ig3 | Run exact NEGBINOM.DIST, boundary and beta-companion batches. Sweep separate p^s and reflected normal-product thresholds over the listed recurrence depths. |
| dc-iuwct.64 | K/ig4 | Run the supplied Touchstone read script for 102 RATE and 32 FV calls from the two frozen JSON files. Compare 55 structural rows plus adjacent timing/crossover cases to 64-delta.tsv. Solver owner dc-oqc1. |
| dc-iuwct.65 | K/ig3 | Run T.DIST.json and 60 high-df density controls. Capture x=0 normalizers and matched POWER, LN and three staged EXP companion answers. |
| dc-iuwct.66 | K/ig3 | Run TDIST.json and five boundary batches across legacy/modern one-/two-tail aliases, with BETA.DIST and ERFC.PRECISE comparisons. Find the first zero by degree and recheck beyond it. |
| dc-kapl.4 | K/om1 | Use a separate owned worker and the same recorded request mix before/after. Measure over 20 warm/post-idle batches, instrument each COM fingerprint phase, and report quantiles. Separately force a drop of the run's own link after initial answers, then a broker restart, and count replay/evaluations. |
| dc-mk7r | tk2 | Run the Windows CalcVBA Sin.6/Sin.8 tests and record the UCRT and worksheet words. Use the remaining conformance cases in dc-q6brh.112 to retain wider arithmetic controls. |
| dc-oqc1 | K/ig4 | Share the RATE/FV overflow crossover capture with dc-iuwct.64. Retain the timing=1 controls and original structural denominator for solver and publication comparisons. |
| dc-q6brh.3 | F/lib1 | Finish live Hijri arithmetic/conversion and unmeasured Load/Unload cases. The existing 315 named cases and constants already have retained evidence. |
| dc-q6brh.21 | F/lib1 | Run live Chr/Asc at Windows ACP 932 and retain argument/result bytes, error identities and the ACP used. Reuse the Linux backends as controls. |
| dc-q6brh.23 | F/lib1 | Capture the 12 remaining CollectionEdges in Excel VBA with the authored cases, then replay VM3 and JIT against those answers. |
| dc-q6brh.31 | F/lib1 | Capture Excel VBA DateSerial.10 in the original regional profile and the comparison profile. Regional cutoff ownership is dc-q6brh.6. |
| dc-q6brh.100 | F/lib1 | Verify the inferred String numeric bounds using the authored Excel VBA boundary probes. Preserve neighboring accepted/refused results and compare both backends. |
| dc-q6brh.112 | F/lib1 | Run the wider trig/Exp predictions in native Excel VBA. Keep the three MathShape Exp discrepancies and signed-zero Sin/Tan observations. |
| dc-8svfs.3 | K/cv1 | Run native Excel probes for path mapping, case-only rename and attributes, including the authored fallback cases. Native macOS verification also belongs to dc-8svfs.3. |
| dc-8svfs.4 | K/cv1 | Run the native Windows HKCU settings path with a fresh scoped application value in separate processes. Compare SaveSetting/GetSetting and deletion against authored Excel VBA. |
| dc-8svfs.5 | K/cv1 | Compare emitted text-file bytes and readback for the exact authored ANSI I/O cases under the configured code page, including Unicode and conversion edge cases. |
| dc-8svfs.6 | K/cv1 | Capture Windows Environ enumeration/case/alias behavior with the prepared authored probe. Record only the scoped controlled variables and omit values from the surrounding process environment. |
| dc-8svfs.9 | K/cv1 | Run the prepared Open/Name nonexistent-parent cases in live Excel VBA. Verify error 76 and original-file preservation against the two CalcVBA backends. |
| dc-ymjf.2.38 | F/bx1 | Replay 432 tripling cases as BETA.INV plus the four p=.49,a=.001 second-shape cases against the saved exact words. Compare the identical legacy-alias control observations. |
| dc-ymjf.3 | tk2 | Use retained drift inputs and selections for the outstanding 84 spill layouts and nine service failures. Record old/new Excel build and CPU, array shape, placement and raw results. Preserve separate populations. |
| dc-ymjf.7 | F/wb2 and tk2 | Replay matched functions on AMD and Intel with the same Excel build and exact input words. Keep the archived Intel study separate. Native Intel bank replay belongs to dc-j9k.62. |
| dc-ypa4 | tk2 | Replay the one remaining MIRR 99-row input using an owned authored worksheet because it exceeds relay array limits. Record live Value2 bits and both cashflow orientations. Retain the 16 verified relay controls. |
| dc-zsn.2 | K/lz1 | Capture remaining Unicode block comparisons in matching regional settings. Include the ignorable and archaic Hangul ordering probes, retaining exact UTF-16 units and locale. |
| dc-zsn.3 | K/lz1 | Run ASC/DBCS and byte-family cases on Japanese, Korean and Chinese Excel. Record installed language and width/byte outputs separately. |
| dc-zsn.4 | K/eb4 and K/lz1 | Finish the single retained en-ZA case and run the authored locale-tagged month/day/era comparisons on appropriate installed Excel locales. Record precise text and active regional settings. |
| dc-zsn.6 | K/lz1 | On en-ZA and a second installed Office language pack, read Formula, Formula2 and FormulaLocal for the authored function and list/decimal separator cases. Include entry/display round trips, preserving exact function spellings. |

## Service, installers and evidence availability

| Bead | Lane handoff | Pending step |
| --- | --- | --- |
| dc-ejvk.7 | tk2 | Recover the original 40 setup/save/reopen inputs and receipts identified in 19236152, then run the exact-original sequence through the owned session. Run only owned authored inputs. Keep the 100 eng19, 80 eng20 and R28 results attached to their earlier provenance. |
| dc-fetch-f0tp.1 | K/image3 and tk2 | Use an owned Excel session with an authored IMAGE fixture. Save successful pictures with a controlled HTTPS source, disconnect it and reopen. Record image size/alt, cell rendering and persisted package fields. Keep scalar-versus-picture control under dc-078b. |
| dc-fk66.2 | tk2 | Run the existing package/install commands for both per-user MSI products in a disposable Windows profile. Record clean install, upgrade, downgrade refusal, uninstall, WebView2 bootstrap and retained settings. Signed clean-machine testing belongs to dc-fk66.3. |
| dc-hc85 | F/eng19 | Recover the retained captures and inputs for the remaining 197/486. Inspect attributable capture refusals and timeouts using existing logs. Reproduce chart/dialog-sheet admission only with owned authored equivalents. |
| dc-kitu | tk2 | Run the existing v2 owned worker with a controlled 24-hour mixed-argument load. Record wait counts, throughput and latency by phase. Fingerprint spikes and reconnect follow-up remain dc-kapl.4. |
| dc-tey4.5 | tk2 | Use retained Excel answers for Wasabi and riff and replay the 12 Wasabi and five riff residuals through the Windows VM3/JIT module paths. Preserve exact syntax, reference binding and errors. Refresh Excel results only for owned authored controls. |
| dc-ztvv.13 | K/om1 | Run the paired default/disabled and trusted comparisons with exact admitted native provider and capture hashes. Use separate directories and an owned authored native fixture. Retain inclusive counts, typed refusals, losses and separately reported saved-cache results. |

## Additional handoffs and shared prerequisites

| Bead | Lane handoff | Pending step |
| --- | --- | --- |
| dc-1nbp.10 | K/px1 | After the grouped scheduler lands, replay the retained Ofgem capture through the offline comparison route with its 209,272-cell denominator. The scheduler and resource refusal remain with dc-1nbp.10. |
| dc-1nbp.19 | tk2, px2 supplied the branch inventory | Run the 30 E7, four E12 and five E13 branches in answer 16's harvest on the private desktop. Share E12/E13 with dc-1nbp.14/.15 and route E6 to dc-1nbp.17. Record dialogs, observed values and exact saved packages. |
| dc-1nbp.30 | tk2, px2 supplied the experiment inventory | Once the owner chooses an approved deployment path, measure J1–J5 from answer 14 on Excel desktop. Trace load/sync, event and custom-function outcomes through the common object model. Approval and scope remain with dc-1nbp.30. |
| dc-6332.18 | tk2 | Replay the cited above-four-ULP gamma and chi-square inverse rows against the same Excel build/CPU, grouped by aliases, shape regimes and numeric versus error responses. Coordinate nonmonotone NUM with dc-6332.18.1. |
| dc-6332.5 | tk2 | Use answer 12 p2,n4 seeded histories and stage-by-stage observations to identify route, initialization and coefficient differences. Reuse the recorded short-history and seasonal controls and coordinate independent holdouts with dc-j9k.61. |
| dc-8ovd | F/gm1 and tk2 | After the owner confirms policy, run the authored trusted/untrusted origin and macro/add-in load controls on the native application paths. Record decisions per runtime and preserve the owner review under dc-8ovd. |
| dc-90ro | tk2 | After shared COM type binding, replay the 45 WithEvents file-route cases with a controlled authored WinHttpRequest event fixture and local test server. Record type resolution and event order. Keep the measured Guarded refusal as its own control. |
| dc-ebb, dc-ebb.4 | cap1 on kode-fi, using cp1 and Harv7 evidence | Run the [Corpus V2 capture batch](#corpus-v2-capture-batch) in the approved Windows Excel VM. It schedules 93 authored formula workbooks, 89 triage-reviewed VBA drivers and cp1’s 300 selected XLSX files. |
| dc-iuwct.41 | tk2 | Use retained ax3 inputs for cubic finite reduction, df=7 tail, over-eight log beta, central asymptotic and inverse publication controls. Re-ask only discriminating exact-word inputs and compare substitutions to the recorded caller results. |
| dc-j9k.6 | tk2 | Reproduce the 18 misses in the 60,000 independent ERF.PRECISE holdout, using the 36a7f3d7 recipe. Add immediate neighbors around 0.5 on both sides and compare words independently of the exact full and pinned snapshot. |
| dc-tey4.7 | tk2 | After dc-yqq0, use the 174a12fa benchmark setup to run the remaining native Excel VBA/VM3/JIT range and worksheet-UDF workloads. Record measured times and call counts, paired with the Linux comparison. |
| dc-uef6.1 | tk2 | Extend the fixed tiny-beta saturation case with independent rounded-down and wider-shape controls across the plateau and normalizer boundary. Keep the existing 774 matching controls. |
| dc-ymjf.2.25 | tk2 | Replay the original 22 BINOM.DIST mass rows and the retained adjacent deviance-series probes. Keep the controls fixed while comparing staged exponent and probability words. Numeric research owner dc-j9k. |
| dc-ymjf.2.26 | tk2 | Replay the two original count-one POISSON.DIST mass inputs by witness ID. Pair each exact input with direct EXP of its staged leaf. Compensation belongs to dc-ymjf.2.26 and direct EXP to dc-ymjf.2.31/dc-j9k.62. |
| dc-ymjf.2.29 | tk2 | Replay the retained BINOM.DIST n=55 zero-success probability inputs and their immediate binary64 predecessors/successors. Record raw probabilities and matched LN/EXP companions for logarithm staging. Arithmetic owner dc-j9k. |
| dc-ymjf.2.31 | tk2 | Use the five exact input sets already reproduced in Excel 20527 to compare direct EXP, Poisson, beta and gamma results on the same build across AMD/Intel where available. Native Intel evidence owner dc-j9k.62. |
| dc-ymjf.2.38.2 | tk2 | Recover the 32 held tiny-second-shape inputs per alias. Replay matching BETA.INV/BETAINV args and compare midpoint versus NUM publication and companion forward CDF. Boundary owner dc-j9k.55. |
| dc-ymjf.2.42 | tk2 | Repeat the serial AMORDEGRC period=1e-100 probe in an owned session. Cycle period 0, 1 and 2 primers, include an unprimed and ordinary integer control and retain repeated words in invocation order. |
| dc-ymjf.2.46 | tk2 | Replay the four remaining AMORDEGRC edge differences in the retained 1,440-call cohort. Keep rate 0.0101010101010101 periods 96–98 and the high-rate period 3 case as distinct discriminants for duration and publication staging. |
| dc-ymjf.2.47 | tk2 | Replay the equal tiny-shape BETA inverse controls archived by eb7, varying the probability around the midpoint and retaining errors as typed results. Solver owner dc-j9k.55. |
| dc-ymjf.2.48 | tk2 | Replay the retained large-chi-square boundary transition controls for CHIINV and CHISQ.INV.RT, using adjacent probabilities/df and recording typed overflow versus finite raw bits. Maximum-degree follow-up dc-ymjf.2.48.1. |
| dc-ymjf.2.66 | tk2 | Reuse the 2,048 paired matrix/vector and isolated TREND controls. Ask the wider GROWTH numeric discriminants with their paired LOGEST factors and final EXP controls on the same build. Retain all six one-bit residuals. |
| dc-ymjf.2.67 | tk2 | Share the fit-versus-EXP replay with dc-ymjf.2.66. Include the original GROWTH residual and the retained adjacent one-ULP plateau. Arithmetic owner dc-j9k. |
| dc-ymjf.4 | tk2 | Use the dedicated answer service for the existing cross-repository race and frozen held-out round. Collect request ownership, frozen predictions, Excel replies and judge counts for the same run. |
| dc-zuwqx.4 | F/fc1 | Run all ten listed April/November underflow formulas, including each divide/rescale and POWER control, in current Excel. Read stored formula and Value2 words and update the linked research notes with the observed build and CPU. |

## Corpus V2 capture batch

After owner approval for both beads, run from DnaCalc master in the isolated Excel VM with
PowerShell 7, Python 3.10+, and the existing R2 client configuration. The authored VBA arm
requires the VM’s already authorized VBA project access. Use a new directory outside the clone.

```powershell
py -3 tools/excel-service/run-corpus-batch.py tools/excel-service/corpus-v2.json --out C:/Work/DnaCalc/corpus/cap1-20261122
```

The batch verifies the staged bytes, starts the private desktop and uses the owned Excel session
client. The workbook arms use the corpus worker with macros force-disabled. The VBA arm imports
each pinned, triage-reviewed driver into its own unsaved blank workbook. Keep the desktop guard,
session receipts, raw captures and both comparison summaries. Inspect the summaries for incomplete
acquisition or comparison. Account for all 150,519 inventoried
cp1 formula cells under dc-ebb. Under dc-ebb.4, map the authored captures to the declared anchors
and spills, fill the expected answers and wire the formula and VBA cases into their existing
corpora. Report missing captures and execution errors alongside pass/fail counts.
