# Excel C API command inventory

The Microsoft Excel 2013 XLL SDK header defines 403 `xlc*` commands. DNA Calc serves one,
`xlcMessage`, and refuses the other 402. The assignment to object-model owners records where
each operation belongs in the [shared model](OBJECT_MODEL.md). Several commands act on the
active sheet, selection, chart or window. Charts and drawing objects are listed as worksheet
subgroups. The owner assignments are architectural judgments.

## Sources and notation

- The ordinal and symbol come from `XLCALL.H` in the [Microsoft Excel 2013 XLL SDK](https://www.microsoft.com/en-us/download/details.aspx?id=35567)
  (SHA-256 `2e0a6907ff3694b9c5ea5b7891086f595d3250d7dce209616b481987705b66cf`).
  The callback number passed to Excel is the ordinal ORed with `xlCommand` (`0x8000`).
  The Rust ABI currently declares the command bit and `XLC_MESSAGE`.
  [Rust ABI](../../crates/calc-xll-abi/src/lib.rs)
- The English XLM names and argument shapes are joined by ordinal against Microsoft's
  [MS-XLSB Cetab](https://learn.microsoft.com/en-nz/openspecs/office_file_formats/MS-XLSB/758caac6-9ea1-4bd4-9a81-b252e675f692).
  `v` denotes a value operand, `r` a reference, and `/` an alternative.
  Brackets indicate optional groups, `*N(...)` permits zero through N repetitions, and
  `()` indicates zero arguments. These are the documented XLM file-formula shapes.
  Defaults and C API behavior on individual Excel builds require separate evidence.
  Cetab lists `MSOCHECKS` (517) outside the SDK header's `xlc*` set.
- Seven SDK ordinals absent from Cetab use named arguments from Microsoft's
  [Excel 4.0 Macro Functions Reference](https://d13ot9o61jdzpp.cloudfront.net/files/Excel-4.0-Macros-Function-Reference.pdf).
  The eighth, `xlcFillWorkgroup`, uses the spelling inferred from the header symbol.
  Its argument list remains unverified. This legacy command and the unimplemented commands
  below fall under the exposed API epic **dc-17spk**.

## DNA Calc dispatch

This is a source inventory of the current Windows host. A status applies to a valid callback
envelope in an active frame. The host also validates operand pointers and count, and has special
handling for free-only frames and an already uncalculated invocation.

| Context | `xlcMessage` | Other SDK commands |
| --- | --- | --- |
| Lifecycle frame | Updates the host status message and returns `TRUE` | Falls through to unsupported callback with status 2 and a refusal diagnostic, for decodable operands |
| Macro-equivalent worksheet entry (`#`) | Updates the host status message and returns `FALSE` | Status 2 and a typed refusal through the invocation |
| Ordinary worksheet entry | Status 2 (`xlretInvXlfn`) | Status 2 (`xlretInvXlfn`) |
| Thread-safe worksheet entry (`$`) | Status 128 (`xlretNotThreadSafe`) | Status 128 (`xlretNotThreadSafe`) |

`xlcMessage` takes one or two operands in DNA Calc. Operand 1 is interpreted as a logical or
as a number (zero clears, nonzero shows). Other decodable types take the clear path. When
showing, the optional second operand supplies the text. Clearing or omitting the text on the
show path leaves no stored message. The operation stores the message in XLL host state.

For lifecycle fallback, the worksheet lookup keeps the command bit set, so none of these
numbers resolve to an ordinary worksheet Ftab entry. The existing source cites Excel build
16.0.20527 measurements for `MESSAGE` and for `ECHO` in a macro-equivalent worksheet entry.
The inventory labels a command **served** when the host implements the operation in at least
one context, **refused** when the source rejects the command in all covered contexts, and
**unknown** when the host outcome cannot be established from the source.
[Callback dispatch](../../crates/calc-xll-host/src/callbacks.rs) ·
[Worksheet fallback](../../crates/calc-xll-host/src/invoke.rs) ·
[Ftab lookup](../../crates/calc-catalogue/src/file_function_ids.rs)

| Owner | SDK commands | Served | Refused | Unknown |
| --- | ---: | ---: | ---: | ---: |
| Application | 82 | 1 | 81 | 0 |
| Workbook | 47 | 0 | 47 | 0 |
| Worksheet | 63 | 0 | 63 | 0 |
| Range | 48 | 0 | 48 | 0 |
| Window | 42 | 0 | 42 | 0 |
| Names | 5 | 0 | 5 | 0 |
| Formatting | 22 | 0 | 22 | 0 |
| Worksheet charts | 55 | 0 | 55 | 0 |
| Worksheet drawing objects and controls | 39 | 0 | 39 | 0 |
| **Total** | **403** | **1** | **402** | **0** |

## Application

| Ordinal | Callback | SDK symbol | XLM command | XLM arguments | DNA Calc |
| ---: | ---: | --- | --- | --- | --- |
| 0 | `0x8000` | `xlcBeep` | `BEEP` | `[v]` | Refused |
| 6 | `0x8006` | `xlcFileDelete` | `FILE.DELETE` | `[v]` | Refused |
| 9 | `0x8009` | `xlcPrinterSetup` | `PRINTER.SETUP` | `[v]` | Refused |
| 10 | `0x800A` | `xlcQuit` | `QUIT` | `()` | Refused |
| 17 | `0x8011` | `xlcRun` | `RUN` | `[(r / v), [v]]` | Refused |
| 30 | `0x801E` | `xlcA1R1c1` | `A1.R1C1` | `[v]` | Refused |
| 31 | `0x801F` | `xlcCalculateNow` | `CALCULATE.NOW` | `()` | Refused |
| 32 | `0x8020` | `xlcCalculation` | `CALCULATION` | `*11(v)` | Refused |
| 48 | `0x8030` | `xlcUndo` | `UNDO` | `()` | Refused |
| 90 | `0x805A` | `xlcEditRepeat` | `EDIT.REPEAT` | `()` | Refused |
| 95 | `0x805F` | `xlcWorkspace` | `WORKSPACE` | `*16(v)` | Refused |
| 118 | `0x8076` | `xlcAlert` | `ALERT` | `*3(v)` | Refused |
| 120 | `0x8078` | `xlcCancelCopy` | `CANCEL.COPY` | `[v]` | Refused |
| 121 | `0x8079` | `xlcShowClipboard` | `SHOW.CLIPBOARD` | `()` | Refused |
| 122 | `0x807A` | `xlcMessage` | `MESSAGE` | `*2(v)` | Served |
| 125 | `0x807D` | `xlcAppActivate` | `APP.ACTIVATE` | `*2(v)` | Refused |
| 131 | `0x8083` | `xlcSendKeys` | `SEND.KEYS` | `*2(v)` | Refused |
| 136 | `0x8088` | `xlcShowInfo` | `SHOW.INFO` | `[v]` | Refused |
| 139 | `0x808B` | `xlcOnData` | `ON.DATA` | `*2(v)` | Refused |
| 140 | `0x808C` | `xlcDisableInput` | `DISABLE.INPUT` | `[v]` | Refused |
| 141 | `0x808D` | `xlcEcho` | `ECHO` | `([logical])` | Refused |
| 148 | `0x8094` | `xlcOnTime` | `ON.TIME` | `*4(v)` | Refused |
| 149 | `0x8095` | `xlcWait` | `WAIT` | `[v]` | Refused |
| 155 | `0x809B` | `xlcShortMenus` | `SHORT.MENUS` | `[v]` | Refused |
| 165 | `0x80A5` | `xlcError` | `ERROR` | `(enable_logical, macro_ref)` | Refused |
| 168 | `0x80A8` | `xlcOnKey` | `ON.KEY` | `*2(v)` | Refused |
| 169 | `0x80A9` | `xlcAppRestore` | `APP.RESTORE` | `()` | Refused |
| 170 | `0x80AA` | `xlcAppMove` | `APP.MOVE` | `*2(v)` | Refused |
| 171 | `0x80AB` | `xlcAppSize` | `APP.SIZE` | `*2(v)` | Refused |
| 172 | `0x80AC` | `xlcAppMinimize` | `APP.MINIMIZE` | `()` | Refused |
| 173 | `0x80AD` | `xlcAppMaximize` | `APP.MAXIMIZE` | `()` | Refused |
| 188 | `0x80BC` | `xlcOpenMail` | `OPEN.MAIL` | `*2(v)` | Refused |
| 189 | `0x80BD` | `xlcSendMail` | `SEND.MAIL` | `[(r / v), *2(v)]` | Refused |
| 220 | `0x80DC` | `xlcShowToolbar` | `SHOW.TOOLBAR` | `*10(v)` | Refused |
| 227 | `0x80E3` | `xlcOnRecalc` | `ON.RECALC` | `*2(v)` | Refused |
| 240 | `0x80F0` | `xlcLinePrint` | `LINE.PRINT` | `*11(v)` | Refused |
| 258 | `0x8102` | `xlcResume` | `RESUME` | `([type_num])` | Refused |
| 265 | `0x8109` | `xlcSoundNote` | `SOUND.NOTE` | `[(r / v), *2(v)]` | Refused |
| 266 | `0x810A` | `xlcSoundPlay` | `SOUND.PLAY` | `[(r / v), *2(v)]` | Refused |
| 276 | `0x8114` | `xlcCustomizeToolbar` | `CUSTOMIZE.TOOLBAR` | `[v]` | Refused |
| 277 | `0x8115` | `xlcAddTool` | `ADD.TOOL` | `*3(v)` | Refused |
| 289 | `0x8121` | `xlcDeleteTool` | `DELETE.TOOL` | `*2(v)` | Refused |
| 290 | `0x8122` | `xlcMoveTool` | `MOVE.TOOL` | `*6(v)` | Refused |
| 293 | `0x8125` | `xlcAssignToTool` | `ASSIGN.TO.TOOL` | `[v, [v, [r / v]]]` | Refused |
| 295 | `0x8127` | `xlcCopyTool` | `COPY.TOOL` | `*2(v)` | Refused |
| 296 | `0x8128` | `xlcResetTool` | `RESET.TOOL` | `*2(v)` | Refused |
| 298 | `0x812A` | `xlcPasteTool` | `PASTE.TOOL` | `*2(v)` | Refused |
| 318 | `0x813E` | `xlcOptionsCalculation` | `OPTIONS.CALCULATION` | `*10(v)` | Refused |
| 319 | `0x813F` | `xlcOptionsEdit` | `OPTIONS.EDIT` | `*11(v)` | Refused |
| 320 | `0x8140` | `xlcOptionsView` | `OPTIONS.VIEW` | `*18(v)` | Refused |
| 321 | `0x8141` | `xlcAddinManager` | `ADDIN.MANAGER` | `*3(v)` | Refused |
| 322 | `0x8142` | `xlcMenuEditor` | `MENU.EDITOR` | `()` | Refused |
| 324 | `0x8144` | `xlcVbaactivate` | `VBAActivate` | `*2(v)` | Refused |
| 325 | `0x8145` | `xlcOptionsChart` | `OPTIONS.CHART` | `*3(v)` | Refused |
| 328 | `0x8148` | `xlcVbaInsertFile` | `VBA.INSERT.FILE` | `[v]` | Refused |
| 330 | `0x814A` | `xlcVbaProcedureDefinition` | `VBA.PROCEDURE.DEFINITION` | `()` | Refused |
| 339 | `0x8153` | `xlcMailLogon` | `MAIL.LOGON` | `[(r / v), [(r /` | Refused |
| 343 | `0x8157` | `xlcEditTool` | `EDIT.TOOL` | `*2(v)` | Refused |
| 355 | `0x8163` | `xlcOptionsTransition` | `OPTIONS.TRANSITION` | `*5(v)` | Refused |
| 356 | `0x8164` | `xlcOptionsGeneral` | `OPTIONS.GENERAL` | `*14(v)` | Refused |
| 375 | `0x8177` | `xlcMailReply` | `MAIL.REPLY` | `()` | Refused |
| 376 | `0x8178` | `xlcMailReplyAll` | `MAIL.REPLY.ALL` | `()` | Refused |
| 377 | `0x8179` | `xlcMailForward` | `MAIL.FORWARD` | `()` | Refused |
| 378 | `0x817A` | `xlcMailNextLetter` | `MAIL.NEXT.LETTER` | `()` | Refused |
| 382 | `0x817E` | `xlcMacroOptions` | `MACRO.OPTIONS` | `*10(v)` | Refused |
| 416 | `0x81A0` | `xlcFullScreen` | `FULL.SCREEN` | `[v]` | Refused |
| 450 | `0x81C2` | `xlcFunctionWizard` | `FUNCTION.WIZARD` | `[v]` | Refused |
| 458 | `0x81CA` | `xlcOptionsListsAdd` | `OPTIONS.LISTS.ADD` | `[v, [r / v]]` | Refused |
| 459 | `0x81CB` | `xlcOptionsListsDelete` | `OPTIONS.LISTS.DELETE` | `[v]` | Refused |
| 467 | `0x81D3` | `xlcMailLogoff` | `MAIL.LOGOFF` | `()` | Refused |
| 469 | `0x81D5` | `xlcAppActivateMicrosoft` | `APP.ACTIVATE.MICROSOFT` | `[v]` | Refused |
| 475 | `0x81DB` | `xlcFindFile` | `FIND.FILE` | `()` | Refused |
| 477 | `0x81DD` | `xlcEnableTipwizard` | `ENABLE.TIPWIZARD` | `[v]` | Refused |
| 478 | `0x81DE` | `xlcVbaMakeAddin` | `VBA.MAKE.ADDIN` | `[v]` | Refused |
| 481 | `0x81E1` | `xlcWorkgroupOptions` | `WORKGROUP.OPTIONS` | `()` | Refused |
| 485 | `0x81E5` | `xlcAutocorrect` | `AUTOCORRECT` | `*2(v)` | Refused |
| 489 | `0x81E9` | `xlcPostDocument` | `POST.DOCUMENT` | `[v]` | Refused |
| 511 | `0x81FF` | `xlcOptionsMenono` | `OPTIONS.MENONO` | `*5(v)` | Refused |
| 647 | `0x8287` | `xlcOptionsMe` | `OPTIONS.ME` | `[(r / v), *8(v)]` | Refused |
| 653 | `0x828D` | `xlcWebPublish` | `WEB.PUBLISH` | `*9(v)` | Refused |
| 753 | `0x82F1` | `xlcOptionsSave` | `OPTIONS.SAVE` | `*4(v)` | Refused |
| 755 | `0x82F3` | `xlcOptionsSpell` | `OPTIONS.SPELL` | `*12(v)` | Refused |

## Workbook

| Ordinal | Callback | SDK symbol | XLM command | XLM arguments | DNA Calc |
| ---: | ---: | --- | --- | --- | --- |
| 1 | `0x8001` | `xlcOpen` | `OPEN` | `*17(v)` | Refused |
| 2 | `0x8002` | `xlcOpenLinks` | `OPEN.LINKS` | `*15(v)` | Refused |
| 3 | `0x8003` | `xlcCloseAll` | `CLOSE.ALL` | `()` | Refused |
| 4 | `0x8004` | `xlcSave` | `SAVE` | `()` | Refused |
| 5 | `0x8005` | `xlcSaveAs` | `SAVE.AS` | `*7(v)` | Refused |
| 16 | `0x8010` | `xlcClose` | `CLOSE` | `*2(v)` | Refused |
| 28 | `0x801C` | `xlcProtectDocument` | `PROTECT.DOCUMENT` | `*7(v)` | Refused |
| 29 | `0x801D` | `xlcPrecision` | `PRECISION` | `[v]` | Refused |
| 119 | `0x8077` | `xlcNew` | `NEW` | `*3(v)` | Refused |
| 144 | `0x8090` | `xlcFileClose` | `FILE.CLOSE` | `*2(v)` | Refused |
| 145 | `0x8091` | `xlcSaveWorkbook` | `SAVE.WORKBOOK` | `*6(v)` | Refused |
| 159 | `0x809F` | `xlcSetUpdateStatus` | `SET.UPDATE.STATUS` | `*3(v)` | Refused |
| 166 | `0x80A6` | `xlcChangeLink` | `CHANGE.LINK` | `*3(v)` | Refused |
| 201 | `0x80C9` | `xlcUpdateLink` | `UPDATE.LINK` | `*2(v)` | Refused |
| 209 | `0x80D1` | `xlcShare` | `SHARE` | `()` | Refused |
| 210 | `0x80D2` | `xlcShareName` | `SHARE.NAME` | `[v]` | Refused |
| 217 | `0x80D9` | `xlcCreatePublisher` | `CREATE.PUBLISHER` | `*4(v)` | Refused |
| 218 | `0x80DA` | `xlcSubscribeTo` | `SUBSCRIBE.TO` | `*2(v)` | Refused |
| 228 | `0x80E4` | `xlcEditSeries` | `EDIT.SERIES` | `[v, *6(r / v)]` | Refused |
| 250 | `0x80FA` | `xlcMergeStyles` | `MERGE.STYLES` | `[v]` | Refused |
| 251 | `0x80FB` | `xlcEditionOptions` | `EDITION.OPTIONS` | `[v, *6(r / v)]` | Refused |
| 281 | `0x8119` | `xlcWorkbookAdd` | `WORKBOOK.ADD` | `*3(v)` | Refused |
| 282 | `0x811A` | `xlcWorkbookMove` | `WORKBOOK.MOVE` | `*3(v)` | Refused |
| 283 | `0x811B` | `xlcWorkbookCopy` | `WORKBOOK.COPY` | `*3(v)` | Refused |
| 284 | `0x811C` | `xlcWorkbookOptions` | `WORKBOOK.OPTIONS` | `*3(v)` | Refused |
| 285 | `0x811D` | `xlcSaveWorkspace` | `SAVE.WORKSPACE` | `[v]` | Refused |
| 302 | `0x812E` | `xlcWorkbookNew` | `WORKBOOK.NEW` | `*3(v)` | Refused |
| 323 | `0x8143` | `xlcAttachToolbars` | `ATTACH.TOOLBARS` | `()` | Refused |
| 336 | `0x8150` | `xlcRoutingSlip` | `ROUTING.SLIP` | `[(r / v), *5(v)]` | Refused |
| 338 | `0x8152` | `xlcRouteDocument` | `ROUTE.DOCUMENT` | `()` | Refused |
| 354 | `0x8162` | `xlcWorkbookInsert` | `WORKBOOK.INSERT` | `[v]` | Refused |
| 373 | `0x8175` | `xlcMailAddMailer` | `MAIL.ADD.MAILER` | `()` | Refused |
| 374 | `0x8176` | `xlcMailDeleteMailer` | `MAIL.DELETE.MAILER` | `()` | Refused |
| 383 | `0x817F` | `xlcWorkbookHide` | `WORKBOOK.HIDE` | `*2(v)` | Refused |
| 384 | `0x8180` | `xlcWorkbookUnhide` | `WORKBOOK.UNHIDE` | `[v]` | Refused |
| 385 | `0x8181` | `xlcWorkbookDelete` | `WORKBOOK.DELETE` | `[v]` | Refused |
| 386 | `0x8182` | `xlcWorkbookName` | `WORKBOOK.NAME` | `*2(v)` | Refused |
| 397 | `0x818D` | `xlcUngroupSheets` | `UNGROUP.SHEETS` | `()` | Refused |
| 417 | `0x81A1` | `xlcWorkbookProtect` | `WORKBOOK.PROTECT` | `*3(v)` | Refused |
| 441 | `0x81B9` | `xlcOpenText` | `OPEN.TEXT` | `*17(v)` | Refused |
| 456 | `0x81C8` | `xlcSaveCopyAs` | `SAVE.COPY.AS` | `[v]` | Refused |
| 468 | `0x81D4` | `xlcClearRoutingSlip` | `CLEAR.ROUTING.SLIP` | `[v]` | Refused |
| 470 | `0x81D6` | `xlcMailEditMailer` | `MAIL.EDIT.MAILER` | `[v, [(r / v), [(r / v), [(r / v), [v, [r / v]]]]]]` | Refused |
| 474 | `0x81DA` | `xlcSummaryInfo` | `SUMMARY.INFO` | `*5(v)` | Refused |
| 482 | `0x81E2` | `xlcMailSendMailer` | `MAIL.SEND.MAILER` | `*2(v)` | Refused |
| 620 | `0x826C` | `xlcProtectRevisions` | `PROTECT.REVISIONS` | `()` | Refused |
| 621 | `0x826D` | `xlcUnprotectRevisions` | `UNPROTECT.REVISIONS` | `()` | Refused |

## Worksheet

| Ordinal | Callback | SDK symbol | XLM command | XLM arguments | DNA Calc |
| ---: | ---: | --- | --- | --- | --- |
| 7 | `0x8007` | `xlcPageSetup` | `PAGE.SETUP` | `*30(v)` | Refused |
| 8 | `0x8008` | `xlcPrint` | `PRINT` | `*17(v)` | Refused |
| 22 | `0x8016` | `xlcSetPrintArea` | `SET.PRINT.AREA` | `[r / v]` | Refused |
| 23 | `0x8017` | `xlcSetPrintTitles` | `SET.PRINT.TITLES` | `*2(r / v)` | Refused |
| 24 | `0x8018` | `xlcSetPageBreak` | `SET.PAGE.BREAK` | `()` | Refused |
| 25 | `0x8019` | `xlcRemovePageBreak` | `REMOVE.PAGE.BREAK` | `*2(v)` | Refused |
| 34 | `0x8022` | `xlcDataFind` | `DATA.FIND` | `[v]` | Refused |
| 35 | `0x8023` | `xlcExtract` | `EXTRACT` | `[v]` | Refused |
| 36 | `0x8024` | `xlcDataDelete` | `DATA.DELETE` | `()` | Refused |
| 37 | `0x8025` | `xlcSetDatabase` | `SET.DATABASE` | `()` | Refused |
| 38 | `0x8026` | `xlcSetCriteria` | `SET.CRITERIA` | `()` | Refused |
| 142 | `0x808E` | `xlcOutline` | `OUTLINE` | `*4(v)` | Refused |
| 146 | `0x8092` | `xlcDataForm` | `DATA.FORM` | `()` | Refused |
| 167 | `0x80A7` | `xlcCalculateDocument` | `CALCULATE.DOCUMENT` | `()` | Refused |
| 199 | `0x80C7` | `xlcWorkgroup` | `WORKGROUP` | `[v]` | Refused |
| 200 | `0x80C8` | `xlcFillGroup` | `FILL.GROUP` | `[v]` | Refused |
| 202 | `0x80CA` | `xlcPromote` | `PROMOTE` | `[v]` | Refused |
| 203 | `0x80CB` | `xlcDemote` | `DEMOTE` | `[v]` | Refused |
| 204 | `0x80CC` | `xlcShowDetail` | `SHOW.DETAIL` | `*4(v)` | Refused |
| 206 | `0x80CE` | `xlcUngroup` | `UNGROUP` | `()` | Refused |
| 216 | `0x80D8` | `xlcSetExtract` | `SET.EXTRACT` | `()` | Refused |
| 222 | `0x80DE` | `xlcPrintPreview` | `PRINT.PREVIEW` | `[v]` | Refused |
| 224 | `0x80E0` | `xlcShowLevels` | `SHOW.LEVELS` | `*2(v)` | Refused |
| 279 | `0x8117` | `xlcOnDoubleclick` | `ON.DOUBLECLICK` | `*2(v)` | Refused |
| 280 | `0x8118` | `xlcOnEntry` | `ON.ENTRY` | `*2(v)` | Refused |
| 301 | `0x812D` | `xlcFillWorkgroup` | `FILL.WORKGROUP` | `unverified` | Refused |
| 305 | `0x8131` | `xlcScenarioCells` | `SCENARIO.CELLS` | `[r / v]` | Refused |
| 306 | `0x8132` | `xlcScenarioDelete` | `SCENARIO.DELETE` | `[v]` | Refused |
| 307 | `0x8133` | `xlcScenarioAdd` | `SCENARIO.ADD` | `[v, [v, [(r / v), *3(v)]]]` | Refused |
| 308 | `0x8134` | `xlcScenarioEdit` | `SCENARIO.EDIT` | `[v, [v, [v, [(r / v), *3(v)]]]]` | Refused |
| 309 | `0x8135` | `xlcScenarioShow` | `SCENARIO.SHOW` | `[v]` | Refused |
| 310 | `0x8136` | `xlcScenarioShowNext` | `SCENARIO.SHOW.NEXT` | `()` | Refused |
| 311 | `0x8137` | `xlcScenarioSummary` | `SCENARIO.SUMMARY` | `[(r / v), [v]]` | Refused |
| 312 | `0x8138` | `xlcPivotTableWizard` | `PIVOT.TABLE.WIZARD` | `[v, [(r / v), [(r / v), *13(v)]]]` | Refused |
| 313 | `0x8139` | `xlcPivotFieldProperties` | `PIVOT.FIELD.PROPERTIES` | `*7(v)` | Refused |
| 314 | `0x813A` | `xlcPivotField` | `PIVOT.FIELD` | `*4(v)` | Refused |
| 315 | `0x813B` | `xlcPivotItem` | `PIVOT.ITEM` | `*4(v)` | Refused |
| 316 | `0x813C` | `xlcPivotAddFields` | `PIVOT.ADD.FIELDS` | `*5(v)` | Refused |
| 352 | `0x8160` | `xlcPivotItemProperties` | `PIVOT.ITEM.PROPERTIES` | `*7(v)` | Refused |
| 370 | `0x8172` | `xlcFilterAdvanced` | `FILTER.ADVANCED` | `[v, [(r / v), [(r / v), [(r / v), [v]]]]]` | Refused |
| 393 | `0x8189` | `xlcAutoOutline` | `AUTO.OUTLINE` | `()` | Refused |
| 398 | `0x818E` | `xlcSubtotalCreate` | `SUBTOTAL.CREATE` | `*6(v)` | Refused |
| 399 | `0x818F` | `xlcSubtotalRemove` | `SUBTOTAL.REMOVE` | `()` | Refused |
| 421 | `0x81A5` | `xlcPivotShowPages` | `PIVOT.SHOW.PAGES` | `*2(v)` | Refused |
| 433 | `0x81B1` | `xlcPivotFieldGroup` | `PIVOT.FIELD.GROUP` | `*4(v)` | Refused |
| 434 | `0x81B2` | `xlcPivotFieldUngroup` | `PIVOT.FIELD.UNGROUP` | `()` | Refused |
| 439 | `0x81B7` | `xlcPivotRefresh` | `PIVOT.REFRESH` | `[v]` | Refused |
| 447 | `0x81BF` | `xlcFilter` | `FILTER` | `*6(v)` | Refused |
| 448 | `0x81C0` | `xlcFilterShowAll` | `FILTER.SHOW.ALL` | `()` | Refused |
| 449 | `0x81C1` | `xlcClearOutline` | `CLEAR.OUTLINE` | `()` | Refused |
| 471 | `0x81D7` | `xlcOnSheet` | `ON.SHEET` | `*3(v)` | Refused |
| 473 | `0x81D9` | `xlcScenarioMerge` | `SCENARIO.MERGE` | `[v]` | Refused |
| 480 | `0x81E0` | `xlcInsertdatatable` | `INSERTDATATABLE` | `[v]` | Refused |
| 493 | `0x81ED` | `xlcViewShow` | `VIEW.SHOW` | `[v]` | Refused |
| 494 | `0x81EE` | `xlcViewDefine` | `VIEW.DEFINE` | `*3(v)` | Refused |
| 495 | `0x81EF` | `xlcViewDelete` | `VIEW.DELETE` | `[v]` | Refused |
| 510 | `0x81FE` | `xlcInsertMapObject` | `INSERT.MAP.OBJECT` | `()` | Refused |
| 520 | `0x8208` | `xlcRmPrintArea` | `RM.PRINT.AREA` | `[r / v]` | Refused |
| 521 | `0x8209` | `xlcClearPrintArea` | `CLEAR.PRINT.AREA` | `()` | Refused |
| 522 | `0x820A` | `xlcAddPrintArea` | `ADD.PRINT.AREA` | `()` | Refused |
| 523 | `0x820B` | `xlcMoveBrk` | `MOVE.BRK` | `*4(v)` | Refused |
| 667 | `0x829B` | `xlcNewwebquery` | `NEWWEBQUERY` | `[v]` | Refused |
| 673 | `0x82A1` | `xlcPivotTableChart` | `PIVOT.TABLE.CHART` | `[v, [(r / v), [(r / v), *13(v)]]]` | Refused |

## Range

| Ordinal | Callback | SDK symbol | XLM command | XLM arguments | DNA Calc |
| ---: | ---: | --- | --- | --- | --- |
| 39 | `0x8027` | `xlcSort` | `SORT` | `[v, [(r / v), [(r / v), [(r / v), [(r / v), [(r / v), [(r / v), *10(v)]]]]]]]` | Refused |
| 40 | `0x8028` | `xlcDataSeries` | `DATA.SERIES` | `*6(v)` | Refused |
| 41 | `0x8029` | `xlcTable` | `TABLE` | `*2(r / v)` | Refused |
| 49 | `0x8031` | `xlcCut` | `CUT` | `*2(r / v)` | Refused |
| 50 | `0x8032` | `xlcCopy` | `COPY` | `*2(r / v)` | Refused |
| 51 | `0x8033` | `xlcPaste` | `PASTE` | `[r / v]` | Refused |
| 52 | `0x8034` | `xlcClear` | `CLEAR` | `[v]` | Refused |
| 53 | `0x8035` | `xlcPasteSpecial` | `PASTE.SPECIAL` | `*7(v)` | Refused |
| 54 | `0x8036` | `xlcEditDelete` | `EDIT.DELETE` | `[v]` | Refused |
| 55 | `0x8037` | `xlcInsert` | `INSERT` | `*2(v)` | Refused |
| 56 | `0x8038` | `xlcFillRight` | `FILL.RIGHT` | `()` | Refused |
| 57 | `0x8039` | `xlcFillDown` | `FILL.DOWN` | `()` | Refused |
| 64 | `0x8040` | `xlcFormulaFind` | `FORMULA.FIND` | `*12(v)` | Refused |
| 91 | `0x805B` | `xlcParse` | `PARSE` | `[v, [r / v]]` | Refused |
| 92 | `0x805C` | `xlcJustify` | `JUSTIFY` | `()` | Refused |
| 96 | `0x8060` | `xlcFormula` | `FORMULA` | `[v, [r / v]]` | Refused |
| 97 | `0x8061` | `xlcFormulaFill` | `FORMULA.FILL` | `[v, [r / v]]` | Refused |
| 98 | `0x8062` | `xlcFormulaArray` | `FORMULA.ARRAY` | `[v, [r / v]]` | Refused |
| 99 | `0x8063` | `xlcDataFindNext` | `DATA.FIND.NEXT` | `()` | Refused |
| 100 | `0x8064` | `xlcDataFindPrev` | `DATA.FIND.PREV` | `()` | Refused |
| 101 | `0x8065` | `xlcFormulaFindNext` | `FORMULA.FIND.NEXT` | `()` | Refused |
| 102 | `0x8066` | `xlcFormulaFindPrev` | `FORMULA.FIND.PREV` | `()` | Refused |
| 108 | `0x806C` | `xlcCopyPicture` | `COPY.PICTURE` | `*3(v)` | Refused |
| 124 | `0x807C` | `xlcPasteLink` | `PASTE.LINK` | `()` | Refused |
| 130 | `0x8082` | `xlcFormulaReplace` | `FORMULA.REPLACE` | `*11(v)` | Refused |
| 151 | `0x8097` | `xlcFillUp` | `FILL.UP` | `()` | Refused |
| 152 | `0x8098` | `xlcFillLeft` | `FILL.LEFT` | `()` | Refused |
| 154 | `0x809A` | `xlcNote` | `NOTE` | `(add_text, cell_ref, start_char, num_chars)` | Refused |
| 191 | `0x80BF` | `xlcConsolidate` | `CONSOLIDATE` | `*5(v)` | Refused |
| 192 | `0x80C0` | `xlcSortSpecial` | `SORT.SPECIAL` | `[v, [v, [(r / v), [(r / v), [(r / v), [(r / v), [(r / v), [(r / v), *6(v)]]]]]]]]` | Refused |
| 198 | `0x80C6` | `xlcGoalSeek` | `GOAL.SEEK` | `*3(r / v)` | Refused |
| 243 | `0x80F3` | `xlcEnterData` | `ENTER.DATA` | `[r / v]` | Refused |
| 252 | `0x80FC` | `xlcPastePicture` | `PASTE.PICTURE` | `()` | Refused |
| 253 | `0x80FD` | `xlcPastePictureLink` | `PASTE.PICTURE.LINK` | `()` | Refused |
| 254 | `0x80FE` | `xlcSpelling` | `SPELLING` | `*6(v)` | Refused |
| 274 | `0x8112` | `xlcFillAuto` | `FILL.AUTO` | `[(r / v), [v]]` | Refused |
| 422 | `0x81A6` | `xlcTextToColumns` | `TEXT.TO.COLUMNS` | `[v, [(r / v), *12(v)]]` | Refused |
| 425 | `0x81A9` | `xlcTracerDisplay` | `TRACER.DISPLAY` | `*2(v)` | Refused |
| 430 | `0x81AE` | `xlcTracerNavigate` | `TRACER.NAVIGATE` | `*3(v)` | Refused |
| 431 | `0x81AF` | `xlcTracerClear` | `TRACER.CLEAR` | `()` | Refused |
| 432 | `0x81B0` | `xlcTracerError` | `TRACER.ERROR` | `()` | Refused |
| 491 | `0x81EB` | `xlcPicklist` | `PICKLIST` | `()` | Refused |
| 545 | `0x8221` | `xlcHidecurrNote` | `HIDECURR.NOTE` | `[(r / v), [v]]` | Refused |
| 546 | `0x8222` | `xlcHideallNotes` | `HIDEALL.NOTES` | `[v]` | Refused |
| 547 | `0x8223` | `xlcDeleteNote` | `DELETE.NOTE` | `[r / v]` | Refused |
| 548 | `0x8224` | `xlcTraverseNotes` | `TRAVERSE.NOTES` | `[(r / v), [v]]` | Refused |
| 549 | `0x8225` | `xlcActivateNotes` | `ACTIVATE.NOTES` | `[(r / v), [v]]` | Refused |
| 808 | `0x8328` | `xlcHideallInkannots` | `HIDEALL.INKANNOTS` | `[v]` | Refused |

## Window

| Ordinal | Callback | SDK symbol | XLM command | XLM arguments | DNA Calc |
| ---: | ---: | --- | --- | --- | --- |
| 11 | `0x800B` | `xlcNewWindow` | `NEW.WINDOW` | `()` | Refused |
| 12 | `0x800C` | `xlcArrangeAll` | `ARRANGE.ALL` | `*4(v)` | Refused |
| 13 | `0x800D` | `xlcWindowSize` | `WINDOW.SIZE` | `*3(v)` | Refused |
| 14 | `0x800E` | `xlcWindowMove` | `WINDOW.MOVE` | `*3(v)` | Refused |
| 15 | `0x800F` | `xlcFull` | `FULL` | `[v]` | Refused |
| 27 | `0x801B` | `xlcDisplay` | `DISPLAY` | `*9(v)` | Refused |
| 63 | `0x803F` | `xlcFormulaGoto` | `FORMULA.GOTO` | `[(r / v), [v]]` | Refused |
| 65 | `0x8041` | `xlcSelectLastCell` | `SELECT.LAST.CELL` | `()` | Refused |
| 66 | `0x8042` | `xlcShowActiveCell` | `SHOW.ACTIVE.CELL` | `()` | Refused |
| 93 | `0x805D` | `xlcHide` | `HIDE` | `()` | Refused |
| 94 | `0x805E` | `xlcUnhide` | `UNHIDE` | `[v]` | Refused |
| 103 | `0x8067` | `xlcActivate` | `ACTIVATE` | `*2(v)` | Refused |
| 104 | `0x8068` | `xlcActivateNext` | `ACTIVATE.NEXT` | `[v]` | Refused |
| 105 | `0x8069` | `xlcActivatePrev` | `ACTIVATE.PREV` | `[v]` | Refused |
| 106 | `0x806A` | `xlcUnlockedNext` | `UNLOCKED.NEXT` | `()` | Refused |
| 107 | `0x806B` | `xlcUnlockedPrev` | `UNLOCKED.PREV` | `()` | Refused |
| 109 | `0x806D` | `xlcSelect` | `SELECT` | `*2(r / v)` | Refused |
| 112 | `0x8070` | `xlcVline` | `VLINE` | `[v]` | Refused |
| 113 | `0x8071` | `xlcHline` | `HLINE` | `[v]` | Refused |
| 114 | `0x8072` | `xlcVpage` | `VPAGE` | `[v]` | Refused |
| 115 | `0x8073` | `xlcHpage` | `HPAGE` | `[v]` | Refused |
| 116 | `0x8074` | `xlcVscroll` | `VSCROLL` | `*2(v)` | Refused |
| 117 | `0x8075` | `xlcHscroll` | `HSCROLL` | `*2(v)` | Refused |
| 132 | `0x8084` | `xlcSelectSpecial` | `SELECT.SPECIAL` | `*3(v)` | Refused |
| 135 | `0x8087` | `xlcFreezePanes` | `FREEZE.PANES` | `*3(v)` | Refused |
| 137 | `0x8089` | `xlcSplit` | `SPLIT` | `*2(v)` | Refused |
| 138 | `0x808A` | `xlcOnWindow` | `ON.WINDOW` | `*2(v)` | Refused |
| 163 | `0x80A3` | `xlcWindowRestore` | `WINDOW.RESTORE` | `[v]` | Refused |
| 164 | `0x80A4` | `xlcWindowMaximize` | `WINDOW.MAXIMIZE` | `[v]` | Refused |
| 187 | `0x80BB` | `xlcSelectEnd` | `SELECT.END` | `[v]` | Refused |
| 256 | `0x8100` | `xlcZoom` | `ZOOM` | `[v]` | Refused |
| 260 | `0x8104` | `xlcWindowMinimize` | `WINDOW.MINIMIZE` | `[v]` | Refused |
| 261 | `0x8105` | `xlcSize` | `SIZE` | `(width, height, window_text)` | Refused |
| 262 | `0x8106` | `xlcMove` | `MOVE` | `(x_pos, y_pos, window_text)` | Refused |
| 291 | `0x8123` | `xlcWorkbookSelect` | `WORKBOOK.SELECT` | `*3(v)` | Refused |
| 292 | `0x8124` | `xlcWorkbookActivate` | `WORKBOOK.ACTIVATE` | `*2(v)` | Refused |
| 412 | `0x819C` | `xlcWorkbookScroll` | `WORKBOOK.SCROLL` | `*2(v)` | Refused |
| 413 | `0x819D` | `xlcWorkbookNext` | `WORKBOOK.NEXT` | `()` | Refused |
| 414 | `0x819E` | `xlcWorkbookPrev` | `WORKBOOK.PREV` | `()` | Refused |
| 415 | `0x819F` | `xlcWorkbookTabSplit` | `WORKBOOK.TAB.SPLIT` | `[v]` | Refused |
| 518 | `0x8206` | `xlcNormal` | `NORMAL` | `()` | Refused |
| 519 | `0x8207` | `xlcLayout` | `LAYOUT` | `()` | Refused |

## Names

| Ordinal | Callback | SDK symbol | XLM command | XLM arguments | DNA Calc |
| ---: | ---: | --- | --- | --- | --- |
| 61 | `0x803D` | `xlcDefineName` | `DEFINE.NAME` | `[v, [(r / v), [(r / v), [(r / v), [(r / v), [(r / v), [v]]]]]]]` | Refused |
| 62 | `0x803E` | `xlcCreateNames` | `CREATE.NAMES` | `*4(v)` | Refused |
| 110 | `0x806E` | `xlcDeleteName` | `DELETE.NAME` | `[v]` | Refused |
| 133 | `0x8085` | `xlcApplyNames` | `APPLY.NAMES` | `*7(v)` | Refused |
| 143 | `0x808F` | `xlcListNames` | `LIST.NAMES` | `()` | Refused |

## Formatting

| Ordinal | Callback | SDK symbol | XLM command | XLM arguments | DNA Calc |
| ---: | ---: | --- | --- | --- | --- |
| 26 | `0x801A` | `xlcFont` | `FONT` | `*2(v)` | Refused |
| 42 | `0x802A` | `xlcFormatNumber` | `FORMAT.NUMBER` | `[v]` | Refused |
| 43 | `0x802B` | `xlcAlignment` | `ALIGNMENT` | `*10(v)` | Refused |
| 44 | `0x802C` | `xlcStyle` | `STYLE` | `*2(v)` | Refused |
| 45 | `0x802D` | `xlcBorder` | `BORDER` | `*27(v)` | Refused |
| 46 | `0x802E` | `xlcCellProtection` | `CELL.PROTECTION` | `*2(v)` | Refused |
| 47 | `0x802F` | `xlcColumnWidth` | `COLUMN.WIDTH` | `[v, *4(r / v)]` | Refused |
| 111 | `0x806F` | `xlcDeleteFormat` | `DELETE.FORMAT` | `[v]` | Refused |
| 127 | `0x807F` | `xlcRowHeight` | `ROW.HEIGHT` | `[v, *3(r / v)]` | Refused |
| 134 | `0x8086` | `xlcReplaceFont` | `REPLACE.FONT` | `*10(v)` | Refused |
| 150 | `0x8096` | `xlcFormatFont` | `FORMAT.FONT` | `*15(v)` | Refused |
| 161 | `0x80A1` | `xlcColorPalette` | `COLOR.PALETTE` | `[v]` | Refused |
| 162 | `0x80A2` | `xlcDeleteStyle` | `DELETE.STYLE` | `[v]` | Refused |
| 190 | `0x80BE` | `xlcStandardFont` | `STANDARD.FONT` | `*9(v)` | Refused |
| 212 | `0x80D4` | `xlcApplyStyle` | `APPLY.STYLE` | `[v]` | Refused |
| 223 | `0x80DF` | `xlcEditColor` | `EDIT.COLOR` | `*4(v)` | Refused |
| 229 | `0x80E5` | `xlcDefineStyle` | `DEFINE.STYLE` | `*14(v)` | Refused |
| 269 | `0x810D` | `xlcFormatAuto` | `FORMAT.AUTO` | `*7(v)` | Refused |
| 381 | `0x817D` | `xlcFontProperties` | `FONT.PROPERTIES` | `*14(v)` | Refused |
| 472 | `0x81D8` | `xlcStandardWidth` | `STANDARD.WIDTH` | `[v]` | Refused |
| 476 | `0x81DC` | `xlcActiveCellFont` | `ACTIVE.CELL.FONT` | `*14(v)` | Refused |
| 509 | `0x81FD` | `xlcSheetBackground` | `SHEET.BACKGROUND` | `*2(v)` | Refused |

## Worksheet charts

| Ordinal | Callback | SDK symbol | XLM command | XLM arguments | DNA Calc |
| ---: | ---: | --- | --- | --- | --- |
| 67 | `0x8043` | `xlcGalleryArea` | `GALLERY.AREA` | `*2(v)` | Refused |
| 68 | `0x8044` | `xlcGalleryBar` | `GALLERY.BAR` | `*2(v)` | Refused |
| 69 | `0x8045` | `xlcGalleryColumn` | `GALLERY.COLUMN` | `*2(v)` | Refused |
| 70 | `0x8046` | `xlcGalleryLine` | `GALLERY.LINE` | `*2(v)` | Refused |
| 71 | `0x8047` | `xlcGalleryPie` | `GALLERY.PIE` | `*2(v)` | Refused |
| 72 | `0x8048` | `xlcGalleryScatter` | `GALLERY.SCATTER` | `*2(v)` | Refused |
| 73 | `0x8049` | `xlcCombination` | `COMBINATION` | `[v]` | Refused |
| 74 | `0x804A` | `xlcPreferred` | `PREFERRED` | `()` | Refused |
| 75 | `0x804B` | `xlcAddOverlay` | `ADD.OVERLAY` | `()` | Refused |
| 76 | `0x804C` | `xlcGridlines` | `GRIDLINES` | `*7(v)` | Refused |
| 77 | `0x804D` | `xlcSetPreferred` | `SET.PREFERRED` | `[v]` | Refused |
| 78 | `0x804E` | `xlcAxes` | `AXES` | `*6(v)` | Refused |
| 79 | `0x804F` | `xlcLegend` | `LEGEND` | `[v]` | Refused |
| 80 | `0x8050` | `xlcAttachText` | `ATTACH.TEXT` | `*3(v)` | Refused |
| 81 | `0x8051` | `xlcAddArrow` | `ADD.ARROW` | `()` | Refused |
| 82 | `0x8052` | `xlcSelectChart` | `SELECT.CHART` | `()` | Refused |
| 83 | `0x8053` | `xlcSelectPlotArea` | `SELECT.PLOT.AREA` | `()` | Refused |
| 84 | `0x8054` | `xlcPatterns` | `PATTERNS` | `*13(v)` | Refused |
| 85 | `0x8055` | `xlcMainChart` | `MAIN.CHART` | `*10(v)` | Refused |
| 86 | `0x8056` | `xlcOverlay` | `OVERLAY` | `*12(v)` | Refused |
| 87 | `0x8057` | `xlcScale` | `SCALE` | `*10(v)` | Refused |
| 88 | `0x8058` | `xlcFormatLegend` | `FORMAT.LEGEND` | `[v]` | Refused |
| 89 | `0x8059` | `xlcFormatText` | `FORMAT.TEXT` | `*11(v)` | Refused |
| 126 | `0x807E` | `xlcDeleteArrow` | `DELETE.ARROW` | `()` | Refused |
| 147 | `0x8093` | `xlcCopyChart` | `COPY.CHART` | `[v]` | Refused |
| 153 | `0x8099` | `xlcDeleteOverlay` | `DELETE.OVERLAY` | `()` | Refused |
| 185 | `0x80B9` | `xlcMainChartType` | `MAIN.CHART.TYPE` | `[v]` | Refused |
| 186 | `0x80BA` | `xlcOverlayChartType` | `OVERLAY.CHART.TYPE` | `[v]` | Refused |
| 193 | `0x80C1` | `xlcGallery3dArea` | `GALLERY.3D.AREA` | `[v]` | Refused |
| 194 | `0x80C2` | `xlcGallery3dColumn` | `GALLERY.3D.COLUMN` | `[v]` | Refused |
| 195 | `0x80C3` | `xlcGallery3dLine` | `GALLERY.3D.LINE` | `[v]` | Refused |
| 196 | `0x80C4` | `xlcGallery3dPie` | `GALLERY.3D.PIE` | `[v]` | Refused |
| 197 | `0x80C5` | `xlcView3d` | `VIEW.3D` | `*6(v)` | Refused |
| 225 | `0x80E1` | `xlcFormatMain` | `FORMAT.MAIN` | `*14(v)` | Refused |
| 226 | `0x80E2` | `xlcFormatOverlay` | `FORMAT.OVERLAY` | `*14(v)` | Refused |
| 249 | `0x80F9` | `xlcGalleryRadar` | `GALLERY.RADAR` | `*2(v)` | Refused |
| 272 | `0x8110` | `xlcGallery3dBar` | `GALLERY.3D.BAR` | `[v]` | Refused |
| 273 | `0x8111` | `xlcGallery3dSurface` | `GALLERY.3D.SURFACE` | `[v]` | Refused |
| 288 | `0x8120` | `xlcChartWizard` | `CHART.WIZARD` | `[v, [(r / v), *12(v)]]` | Refused |
| 344 | `0x8158` | `xlcGalleryDoughnut` | `GALLERY.DOUGHNUT` | `*2(v)` | Refused |
| 350 | `0x815E` | `xlcChartTrend` | `CHART.TREND` | `*8(v)` | Refused |
| 379 | `0x817B` | `xlcDataLabel` | `DATA.LABEL` | `*10(v)` | Refused |
| 380 | `0x817C` | `xlcInsertTitle` | `INSERT.TITLE` | `*5(v)` | Refused |
| 388 | `0x8184` | `xlcGalleryCustom` | `GALLERY.CUSTOM` | `[v]` | Refused |
| 390 | `0x8186` | `xlcAddChartAutoformat` | `ADD.CHART.AUTOFORMAT` | `*2(v)` | Refused |
| 391 | `0x8187` | `xlcDeleteChartAutoformat` | `DELETE.CHART.AUTOFORMAT` | `[v]` | Refused |
| 392 | `0x8188` | `xlcChartAddData` | `CHART.ADD.DATA` | `[v, [(r / v), *4(v)]]` | Refused |
| 423 | `0x81A7` | `xlcFormatCharttype` | `FORMAT.CHARTTYPE` | `*4(v)` | Refused |
| 460 | `0x81CC` | `xlcSeriesAxes` | `SERIES.AXES` | `[v]` | Refused |
| 461 | `0x81CD` | `xlcSeriesX` | `SERIES.X` | `[r / v]` | Refused |
| 462 | `0x81CE` | `xlcSeriesY` | `SERIES.Y` | `*2(r / v)` | Refused |
| 463 | `0x81CF` | `xlcErrorbarX` | `ERRORBAR.X` | `[v, [v, [v, [r / v]]]]` | Refused |
| 464 | `0x81D0` | `xlcErrorbarY` | `ERRORBAR.Y` | `[v, [v, [v, [r / v]]]]` | Refused |
| 465 | `0x81D1` | `xlcFormatChart` | `FORMAT.CHART` | `[(r / v), *17(v)]` | Refused |
| 466 | `0x81D2` | `xlcSeriesOrder` | `SERIES.ORDER` | `*3(v)` | Refused |

## Worksheet drawing objects and controls

| Ordinal | Callback | SDK symbol | XLM command | XLM arguments | DNA Calc |
| ---: | ---: | --- | --- | --- | --- |
| 128 | `0x8080` | `xlcFormatMove` | `FORMAT.MOVE` | `[v, *2(r / v)]` | Refused |
| 129 | `0x8081` | `xlcFormatSize` | `FORMAT.SIZE` | `[v, *2(r / v)]` | Refused |
| 174 | `0x80AE` | `xlcBringToFront` | `BRING.TO.FRONT` | `()` | Refused |
| 175 | `0x80AF` | `xlcSendToBack` | `SEND.TO.BACK` | `()` | Refused |
| 207 | `0x80CF` | `xlcObjectProperties` | `OBJECT.PROPERTIES` | `*2(v)` | Refused |
| 208 | `0x80D0` | `xlcSaveNewObject` | `SAVE.NEW.OBJECT` | `[v]` | Refused |
| 211 | `0x80D3` | `xlcDuplicate` | `DUPLICATE` | `()` | Refused |
| 213 | `0x80D5` | `xlcAssignToObject` | `ASSIGN.TO.OBJECT` | `[r / v]` | Refused |
| 214 | `0x80D6` | `xlcObjectProtection` | `OBJECT.PROTECTION` | `*2(v)` | Refused |
| 215 | `0x80D7` | `xlcHideObject` | `HIDE.OBJECT` | `*2(v)` | Refused |
| 219 | `0x80DB` | `xlcAttributes` | `ATTRIBUTES` | `*2(v)` | Refused |
| 259 | `0x8103` | `xlcInsertObject` | `INSERT.OBJECT` | `[v, [v, [v, [v, [v, [v, [v, [(r / v), [v, [v, [(r / v), *2(v)]]]]]]]]]]]` | Refused |
| 267 | `0x810B` | `xlcFormatShape` | `FORMAT.SHAPE` | `[v, [v, [(r / v), *2(v)]]]` | Refused |
| 268 | `0x810C` | `xlcExtendPolygon` | `EXTEND.POLYGON` | `[v]` | Refused |
| 278 | `0x8116` | `xlcEditObject` | `EDIT.OBJECT` | `[v]` | Refused |
| 297 | `0x8129` | `xlcConstrainNumeric` | `CONSTRAIN.NUMERIC` | `[v]` | Refused |
| 300 | `0x812C` | `xlcPlacement` | `PLACEMENT` | `(placement_type)` | Refused |
| 342 | `0x8156` | `xlcInsertPicture` | `INSERT.PICTURE` | `*2(v)` | Refused |
| 394 | `0x818A` | `xlcTabOrder` | `TAB.ORDER` | `()` | Refused |
| 395 | `0x818B` | `xlcShowDialog` | `SHOW.DIALOG` | `[v]` | Refused |
| 396 | `0x818C` | `xlcSelectAll` | `SELECT.ALL` | `()` | Refused |
| 400 | `0x8190` | `xlcRenameObject` | `RENAME.OBJECT` | `[v]` | Refused |
| 420 | `0x81A4` | `xlcScrollbarProperties` | `SCROLLBAR.PROPERTIES` | `*7(v)` | Refused |
| 424 | `0x81A8` | `xlcLinkFormat` | `LINK.FORMAT` | `()` | Refused |
| 435 | `0x81B3` | `xlcCheckboxProperties` | `CHECKBOX.PROPERTIES` | `*5(v)` | Refused |
| 436 | `0x81B4` | `xlcLabelProperties` | `LABEL.PROPERTIES` | `*3(v)` | Refused |
| 437 | `0x81B5` | `xlcListboxProperties` | `LISTBOX.PROPERTIES` | `*5(v)` | Refused |
| 438 | `0x81B6` | `xlcEditboxProperties` | `EDITBOX.PROPERTIES` | `*4(v)` | Refused |
| 440 | `0x81B8` | `xlcLinkCombo` | `LINK.COMBO` | `[v]` | Refused |
| 442 | `0x81BA` | `xlcHideDialog` | `HIDE.DIALOG` | `[v]` | Refused |
| 443 | `0x81BB` | `xlcSetDialogFocus` | `SET.DIALOG.FOCUS` | `[v]` | Refused |
| 444 | `0x81BC` | `xlcEnableObject` | `ENABLE.OBJECT` | `*2(v)` | Refused |
| 445 | `0x81BD` | `xlcPushbuttonProperties` | `PUSHBUTTON.PROPERTIES` | `*6(v)` | Refused |
| 446 | `0x81BE` | `xlcSetDialogDefault` | `SET.DIALOG.DEFAULT` | `[v]` | Refused |
| 451 | `0x81C3` | `xlcAddListItem` | `ADD.LIST.ITEM` | `*2(v)` | Refused |
| 452 | `0x81C4` | `xlcSetListItem` | `SET.LIST.ITEM` | `*2(v)` | Refused |
| 453 | `0x81C5` | `xlcRemoveListItem` | `REMOVE.LIST.ITEM` | `*2(v)` | Refused |
| 454 | `0x81C6` | `xlcSelectListItem` | `SELECT.LIST.ITEM` | `*2(v)` | Refused |
| 455 | `0x81C7` | `xlcSetControlValue` | `SET.CONTROL.VALUE` | `[v]` | Refused |
