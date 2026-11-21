# Skin internals at the end of dc-xfrhg.1

The page imports the model's message definitions through `dna-calc-protocol`. DNA Calc
uses the same page in the browser and WebView2. The Formula Explorer has its own
formula-only session and wire types, and shares the formula field and value display in
`dna-formula-ui`.

| Route | Entry and handler |
|---|---|
| Browser | `page/port.rs:173` sends `ToServer` (`model/projection.rs:823`); `worker/worker.rs:85` unpacks it and calls `worker/server.rs:5119`. The worker sends events and replies; `page/port.rs:358` routes them to the controller and pending callbacks. |
| Desktop | `apps/dna-calc/web/desktop-bridge.js:63` adapts WebView2 to the worker interface and wraps binary payloads in base64. `desktop/app.rs:377,462` serves native commands and forwards workbook traffic to the same server. |
| Headless tests | `worker/transport.rs:73` delivers typed requests, replies and events through the same server. |
| Formula Explorer browser | `one/surface.rs:306` submits authored state over `one/port.rs:533`'s local link. Formula editing and display use `apps/dna-formula-ui/src/lib.rs`. |
| Formula Explorer desktop | `one/bridge.rs:18` wraps requests in `Call` and replies in `Answer`. `one-desktop/app.rs:862` dispatches to the local host or the add-in surrogate. |
| Formula Explorer events | `one/bridge.rs:24` carries a string kind. The desktop emits `rtd`, `addin-process` and `addin-process-stopped` in `one-desktop/app.rs:777,854,910` and `one-desktop/surrogate.rs:479`. The bridge routes them to `one/surface.rs:327`. |

## Skin entry points

The registry is at `page/app.rs:34`. Every skin gets a `Controller` from `page/skin.rs:54`.
It keeps workbook and selection state through skin switches. The controller receives
events at `page/controller.rs:733`, retires state on workbook install at
`page/controller.rs:981`, and separately subscribes to the session selection at
`page/controller.rs:4207`. Shell commands enter at `page/controller.rs:4652`;
skin edits enter at `page/controller.rs:1308`.

| Skin | Projection request and sender |
|---|---|
| Grid | `Geometry` at `page/grid/mod.rs:196`, viewport tiles at `page/grid/mod.rs:521`. |
| Notebook | `Notebook` and `BlockDetail` at `page/notebook.rs:165,278`. |
| Atlas | `Atlas` and `AtlasRefusals` at `page/atlas.rs:64,125`. |
| Tree | `Outline` and `Dependents` at `page/tree.rs:224,301`. |
| Canvas | `Canvas` at `page/canvas.rs:170`. |
| Ledger | `Ledger` at `page/ledger.rs:167`. |

## Multiple handling paths

| Path | Copies and owner |
|---|---|
| Native Open | The desktop checks the same envelope for a startup file, for the native picker, and for request substitution, then the server decodes it to dispatch. See `desktop/app.rs:233,377,396`, `desktop/origin.rs:14`, and `worker/server.rs:5119`. dc-xfrhg.4 owns this route. |
| Native add-ins | `desktop/app.rs:461` decodes page traffic to intercept list/load/unload and builds the reply. The server has browser refusals for the same requests. dc-xfrhg.4 owns the shared command path. |
| Tile messages | `worker/worker.rs:36` and `desktop/app.rs:265` both select the binary tile codec and wrap control events. `page/port.rs` receives both implementations through one handler. The headless adapter delivers typed events. dc-xfrhg.4 owns the desktop transport boundary. |
| Formula Explorer | `one-desktop/app.rs:819,852,862` inspects stop and add-in list calls and decodes remote calls; `one-desktop/host.rs:276` decodes local calls. The browser local link takes typed requests. Host and surface both handle file and add-in outcomes in their respective sessions. The Formula Explorer port is outside this epic and belongs to brief item 9. |
| Other projection work | The current worker builds per-skin projections. dc-xfrhg.5 owns shared worker code. Reply visibility and publication resource costs are tracked by dc-1nbp.21 and dc-1nbp.20. See the [review harvest](../architecture/review-2026-11/pro/harvest/17-harvest.md) for the existing subscription and tile-mirror guarantees. |

## Deletions

DNA Calc intents 70 → 70, requests 74 → 73, replies 49 → 49, events 7 → 7.
Formula Explorer requests 35 → 32, replies 17 → 17. DNA Calc's `CancelSave`
had only a handler. The Formula Explorer's `SetForm`, `RemoveInput` and
`ClearInputs` had only handlers. The surface already submits the whole authored
formula and input state in `Evaluate`. The page now returns local transport
errors directly, removing the second page-side decoding match. The table
includes requests exercised through tests, including calculated save and scoped
calculation.

## Message index

Paths are relative to each alias's `src/` unless they begin with `tests/`.
Line numbers refer to the dc-xfrhg.1 tree. “Test” marks a test reference. A dash means the route has
no indexed Rust reference on that side. Smoke and diagnostic JavaScript uses the same envelopes. The generic transport sends
and receives all request/reply variants through the routes above.

| Alias | Source |
|---|---|
| page | `apps/dna-calc` |
| worker | `apps/dna-calc-worker` |
| desktop | `apps/dna-calc-desktop` |
| model | `apps/dna-calc-model` |
| protocol | `apps/dna-calc-protocol` |
| one | `apps/dna-onecalc` |
| one-desktop | `apps/dna-onecalc-desktop` |
| harness | `tools/xtask-harness` |

### DNA Calc Intent (70)

Declarations at `model/projection.rs:294`.

| Message | Page or client | Worker or desktop |
|---|---|---|
| `RefreshAll` (295) | `page/pivots.rs:189` | `worker/server.rs:2308` |
| `PivotCreate` (298) | `page/pivots.rs:126` | `worker/server.rs:2311` |
| `PivotRefresh` (306) | `page/pivots.rs:378` | `worker/server.rs:2326` |
| `PageSetup` (311) | `page/page_setup.rs:468` | `worker/server.rs:2416` |
| `ChartCreate` (316) | `page/charts.rs:122` | `worker/server.rs:2335` |
| `ChartSeriesStyle` (324) | `page/charts.rs:449` | `worker/server.rs:2350` |
| `ChartElementTextStyle` (332) | `page/charts.rs:299` | `worker/server.rs:2365` |
| `ChartTextStyle` (339) | `page/charts.rs:292` | `worker/server.rs:2384` |
| `ChartType` (345) | `page/charts.rs:368` | `worker/server.rs:2401` |
| `ChartLegend` (353) | `page/charts.rs:673` | `worker/server.rs:2578` |
| `ShapePlace` (360) | `page/grid/mod.rs:2732` | `worker/server.rs:2425` |
| `ShapeDelete` (370) | `page/grid/mod.rs:2785` | `worker/server.rs:2433` |
| `ShapeAdd` (377) | `page/grid/shape_tools.rs:76` | `worker/server.rs:2438` |
| `SparklineAdd` (388) | `page/grid/sparkline_tools.rs:35` | `worker/server.rs:2463` |
| `SparklineKind` (396) | `page/grid/sparkline_tools.rs:47` | `worker/server.rs:2475` |
| `SparklineShow` (403) | `page/grid/sparkline_tools.rs:60` | `worker/server.rs:2480` |
| `SparklineDelete` (411) | `page/grid/sparkline_tools.rs:69` | `worker/server.rs:2485` |
| `ShapePicture` (418) | `page/grid/shape_tools.rs:224` | `worker/server.rs:2490` |
| `ShapeText` (425) | `page/grid/mod.rs:2708`, `page/grid/shape_tools.rs:119` | `worker/server.rs:2511` |
| `ShapeFill` (432) | `page/grid/shape_tools.rs:91` | `worker/server.rs:2516` |
| `ShapeLine` (440) | `page/grid/shape_tools.rs:94` | `worker/server.rs:2524` |
| `ShapeRotate` (448) | `page/grid/shape_tools.rs:102` | `worker/server.rs:2538` |
| `ShapeDuplicate` (455) | `page/grid/mod.rs:2760`, `page/grid/shape_tools.rs:161` | `worker/server.rs:2553` |
| `ShapeNudge` (463) | `page/grid/mod.rs:2776` | `worker/server.rs:2558` |
| `ShapeFlip` (471) | `page/grid/shape_tools.rs:156,157` | `worker/server.rs:2563` |
| `ShapeOrder` (478) | `page/grid/shape_tools.rs:103` | `worker/server.rs:2543` |
| `ShapeGroup` (485) | `page/grid/shape_tools.rs:173` | `worker/server.rs:2568` |
| `ShapeUngroup` (491) | `page/grid/shape_tools.rs:177` | `worker/server.rs:2573` |
| `ChartLogAxis` (496) | `page/charts.rs:589` | `worker/server.rs:2589` |
| `ChartAxis` (503) | `page/charts.rs:634` | `worker/server.rs:2602` |
| `ChartSeries` (513) | `page/charts.rs:499` | `worker/server.rs:2621` |
| `ChartProperties` (524) | `page/charts.rs:711` | `worker/server.rs:2642` |
| `CreateReview` (532) | `page/review.rs:678` | `worker/server.rs:2192` |
| `FormatReview` (541) | `page/review.rs:376` | `worker/server.rs:2211` |
| `ResolveReview` (550) | `page/review.rs:510` | `worker/server.rs:2236` |
| `EditReview` (556) | `page/review.rs:688` | `worker/server.rs:2246` |
| `EditName` (562) | `page/names.rs:154` | `worker/server.rs:2257` |
| `FormatCells` (569) | `page/format.rs:259` | `worker/server.rs:2272` |
| `NotebookLink` (574) | `page/notebook.rs:357,467` | `worker/server.rs:2298` |
| `NotebookOrder` (579) | `page/notebook.rs:379,441` | `worker/server.rs:2303` |
| `Replace` (584) | `page/commands.rs:278` | `worker/server.rs:2864` |
| `EditConditional` (590) | `page/review.rs:285` | `worker/server.rs:2875` |
| `AcceptValidation` (596) | `page/controller.rs:1411` | `worker/server.rs:2285` |
| `ShiftCells` (599) | `page/controller.rs:3967` | `worker/server.rs:2886` |
| `EditSheet` (604) | `page/controller.rs:2325` | `worker/server.rs:2895` |
| `FitContents` (608) | `page/controller.rs:2709` | `worker/server.rs:2901` |
| `Outline` (614) | `page/controller.rs:2466` | `worker/server.rs:2912` |
| `AxisFormat` (620) | `page/controller.rs:2560` | `worker/server.rs:2947` |
| `SplitPanes` (628) | `page/controller.rs:3214` | `worker/server.rs:2962` |
| `FreezePanes` (636) | `page/controller.rs:3282` | `worker/server.rs:2977` |
| `Table` (641) | `page/tables.rs:57` | `worker/server.rs:2657` |
| `AutoFilter` (645) | `page/controller.rs:2782` | `worker/server.rs:2712` |
| `SortRowsWithOptions` (651) | `page/controller.rs:3158` | `worker/server.rs:2830` |
| `SortRows` (659) | `page/controller.rs:3167` | `worker/server.rs:2850` |
| `ApplyPaste` (665) | `page/controller.rs:2063` | `worker/server.rs:2986` |
| `EnterCell` (670) | `page/controller.rs:3875,3896` | `worker/server.rs:3232` |
| `EnterSelection` (674) | `page/controller.rs:3873` | `worker/server.rs:3236` |
| `Clear` (677) | `page/controller.rs:4007` | `worker/server.rs:3237` |
| `Navigate` (680) | `page/controller.rs:1342,1351,1585` | `worker/server.rs:2999` |
| `EditAxis` (686) | `page/controller.rs:3940,3972` | `worker/server.rs:3243` |
| `Copy` (693) | `page/controller.rs:2188` | `worker/server.rs:3261` |
| `Undo` (698) | `page/controller.rs:1342,4682` | `worker/server.rs:3266` |
| `Redo` (699) | `page/controller.rs:1342,4682` | `worker/server.rs:3267` |
| `Recalculate` (700) | `page/controller.rs:4687` | `worker/server.rs:3268` |
| `Rebuild` (701) | — | `worker/server.rs:3269` `worker/server.rs:180` (test) |
| `CalculateRange` (702) |  `page/tests/workbook.rs:53` (test) | `worker/server.rs:3291` |
| `Calculate` (705) |  `page/tests/workbook.rs:235` (test) | `worker/server.rs:3270` |
| `SetCalcMode` (709) | `page/controller.rs:4159` | `worker/server.rs:3297` |
| `SetIteration` (714) | `page/controller.rs:4050` | `worker/server.rs:3302` |
| `Select` (719) | `page/controller.rs:1092,1342,1351,1540,1602` | `worker/server.rs:3315` |

### DNA Calc Request (73)

Declarations at `model/projection.rs:956`.

| Message | Page or client | Worker or desktop |
|---|---|---|
| `CellImage` (958) | `page/grid/image.rs:30` | `worker/server.rs:514,1432` |
| `ShapePicture` (963) | `page/grid/image.rs:82` | `worker/server.rs:517,1433` |
| `PageSetup` (967) | `page/page_setup.rs:406` | `worker/server.rs:1390` |
| `PrintPreview` (968) | `page/printing.rs:706` | `worker/server.rs:1393` |
| `InspectTemplate` (972) | `page/controller.rs:2212` | `worker/server.rs:823` |
| `OpenTemplate` (973) | `page/controller.rs:1239` | `desktop/origin.rs:28`, `worker/server.rs:860,861,866` |
| `InsertTemplate` (976) | `page/controller.rs:2252` | `worker/server.rs:843` |
| `FilterValues` (980) | `page/controller.rs:2848` | `worker/server.rs:1236` |
| `Names` (985) | `page/names.rs:109` | `worker/server.rs:635` |
| `EvaluateName` (989) | `page/names.rs:232` | `worker/server.rs:638` |
| `EvaluatePart` (996) | `page/controller.rs:494` | `worker/server.rs:648` |
| `TraceFormula` (1003) | `page/controller.rs:3478` | `worker/server.rs:663` |
| `UserFunctions` (1009) | `page/controller.rs:3550` | `worker/server.rs:672` |
| `Addins` (1011) | `page/addins.rs:19` | `desktop/app.rs:466`, `worker/server.rs:677` |
| `LoadAddinNative` (1013) | `page/addins.rs:46` | `desktop/app.rs:467,472`, `worker/server.rs:678` |
| `LoadAddin` (1015) | `page/addins.rs:54` | `desktop/app.rs:467,483`, `worker/server.rs:679` |
| `UnloadAddin` (1019) | `page/addins.rs:138` | `desktop/app.rs:467,484`, `worker/server.rs:680` |
| `CalculateName` (1022) | — | `worker/server.rs:681` `worker/names.rs:426` (test) |
| `PollNameCalculation` (1027) | — | `worker/server.rs:720` `worker/names.rs:440` (test) |
| `CancelNameCalculation` (1030) | — | `worker/server.rs:728` `worker/names.rs:583` (test) |
| `FormatSelection` (1033) | `page/format.rs:190` | `worker/server.rs:745` |
| `PreviewFormat` (1034) | `page/format.rs:227` | `worker/server.rs:752` |
| `Summary` (1039) | `page/controller.rs:514` | `worker/server.rs:763` |
| `Command` (1040) | `page/controller.rs:682`, `page/port.rs:231` | `desktop/app.rs:397`, `desktop/origin.rs:21`, `worker/server.rs:511,532,764` |
| `Find` (1041) | `page/commands.rs:203` | `worker/server.rs:765` |
| `SelectionStats` (1042) | `page/commands.rs:739` | `worker/server.rs:768` |
| `CanEdit` (1043) | `page/commands.rs:359` | `worker/server.rs:771` |
| `FitContents` (1044) | `page/controller.rs:2689` | `worker/server.rs:774` |
| `Capture` (1047) | `page/controller.rs:1695,1818` | `worker/server.rs:775` |
| `PreparePaste` (1051) | `page/controller.rs:2014` | `worker/server.rs:814` |
| `Ping` (1059) | `page/port.rs:121` | `worker/server.rs:820` |
| `New` (1061) | `page/controller.rs:1173` | `desktop/origin.rs:28`, `worker/server.rs:854` |
| `OpenNative` (1063) | `page/controller.rs:1237` | `desktop/app.rs:398`, `desktop/origin.rs:24`, `worker/server.rs:851` |
| `Open` (1065) | `page/controller.rs:1243` | `desktop/origin.rs:33`, `worker/server.rs:858,864` |
| `OpenForEditing` (1069) | `page/controller.rs:1241` | `worker/server.rs:859,862,865` |
| `LoadVbaModule` (1073) |  `page/tests/workbook.rs:2761` (test) | `worker/server.rs:928` |
| `LoadVbaProject` (1079) |  `page/tests/workbook.rs:2807` (test) | `worker/server.rs:937` |
| `SetVbaEventsEnabled` (1083) |  `page/tests/workbook.rs:2859` (test) | `worker/server.rs:942` |
| `EnableMacros` (1089) | `page/controller.rs:4120` | `worker/server.rs:946` |
| `UnblockMacros` (1092) | `page/controller.rs:4121` | `worker/server.rs:946,947` |
| `SetMacroSecurity` (1095) | `page/controller.rs:4095` | `worker/server.rs:958` |
| `Save` (1099) |  `page/tests/workbook.rs:2132` (test) | `worker/server.rs:962` |
| `SaveWithContext` (1102) | `page/controller.rs:4398` | `worker/server.rs:963,971,975,979` |
| `SaveAs` (1109) | `page/controller.rs:4393` | `worker/server.rs:964,980,985` |
| `Intent` (1114) | `page/controller.rs:1356` | `worker/server.rs:1117` |
| `Subscribe` (1116) | `page/controller.rs:4207` | `worker/server.rs:1123` |
| `SetInterest` (1121) | `page/controller.rs:4220` | `worker/server.rs:1157` |
| `Unsubscribe` (1126) | `page/controller.rs:4234`, `page/port.rs:216` | `worker/server.rs:1223` |
| `TileAck` (1131) | `page/controller.rs:956`, `page/port.rs:182,190,216` | `worker/server.rs:1181` |
| `ResetTiles` (1133) | `page/controller.rs:965`, `page/port.rs:216` | `worker/server.rs:1189` |
| `Review` (1137) | `page/review.rs:223` | `worker/server.rs:1360` |
| `ReviewMarkers` (1138) | `page/grid/mod.rs:2981` | `worker/server.rs:1361` |
| `Geometry` (1140) | `page/grid/mod.rs:196` | `worker/server.rs:1329` |
| `UsedArea` (1141) | `page/grid/mod.rs:341` | `worker/server.rs:1330` |
| `ResolveReference` (1142) | `page/controller.rs:2113,3389` | `worker/server.rs:1333` |
| `Charts` (1147) | `page/charts.rs:69,136`, `page/grid/mod.rs:352` | `worker/server.rs:1428` |
| `Shapes` (1149) | `page/grid/mod.rs:362` | `worker/server.rs:1429` |
| `Sparklines` (1151) | `page/grid/mod.rs:372` | `worker/server.rs:1430` |
| `Controls` (1153) | `page/grid/mod.rs:382` | `worker/server.rs:1431` |
| `PivotTables` (1154) | `page/pivots.rs:25` | `worker/server.rs:1424` |
| `PivotSourceFields` (1155) | `page/pivots.rs:79` | `worker/server.rs:1415` |
| `Validation` (1160) | `page/grid/mod.rs:3121` | `worker/server.rs:1537` |
| `ValidationInvalid` (1161) | `page/review.rs:49` | `worker/server.rs:1521` |
| `Atlas` (1163) | `page/atlas.rs:64` | `worker/server.rs:1438` |
| `AtlasRefusals` (1165) | `page/atlas.rs:125` | `worker/server.rs:1434` |
| `Outline` (1171) | `page/controller.rs:3499`, `page/tree.rs:224` | `worker/server.rs:1459` |
| `Notebook` (1177) | `page/notebook.rs:165` | `worker/server.rs:1470` |
| `BlockDetail` (1183) | `page/app.rs:274`, `page/notebook.rs:278` | `worker/server.rs:1481` |
| `Dependents` (1185) | `page/tree.rs:301` | `worker/server.rs:1484` |
| `Canvas` (1187) | `page/canvas.rs:170` | `worker/server.rs:1495` |
| `Ledger` (1194) | `page/ledger.rs:167` | `worker/server.rs:1505` |
| `SaveCalculated` (1203) | — | `worker/server.rs:1060` `worker/server.rs:8173` (test) |
| `PollSave` (1204) | `page/controller.rs:768` | `worker/server.rs:1076` |

### DNA Calc Reply (49)

Declarations at `model/projection.rs:1704`.

| Message | Page or client | Worker or desktop |
|---|---|---|
| `OpenCancelled` (1706) | `page/controller.rs:1289` | `desktop/app.rs:421` |
| `CellImage` (1707) | `page/grid/image.rs:31` | `worker/server.rs:1579,1597,1602` |
| `ShapePicture` (1709) | `page/grid/image.rs:83` | `worker/server.rs:1569,1570` |
| `PageSetup` (1710) | `page/page_setup.rs:416,425` | `worker/server.rs:1390` |
| `PrintPreview` (1711) | `page/printing.rs:721,750` | `worker/server.rs:1394` |
| `TemplateFormat` (1712) | `page/controller.rs:2214` | `worker/server.rs:841` |
| `FilterValues` (1713) | `page/controller.rs:2864,2892` | `worker/server.rs:1240` |
| `Names` (1718) | `page/names.rs:117` | `worker/server.rs:636` |
| `NameEvaluation` (1719) | `page/names.rs:245` | `worker/server.rs:607,642` |
| `Part` (1720) | `page/controller.rs:502` | `worker/server.rs:653` |
| `Trace` (1721) | `page/controller.rs:3483` | `worker/server.rs:665` |
| `UserFunctions` (1722) | `page/controller.rs:3554` | `worker/server.rs:673` |
| `Addins` (1723) | `page/addins.rs:20,30` | `desktop/app.rs:491`, `worker/server.rs:677` |
| `NameCalculation` (1724) | — | `worker/server.rs:610,713,726,738` |
| `FormatSelection` (1725) | `page/format.rs:195,204` | `worker/server.rs:745` |
| `FormatPreview` (1726) | `page/format.rs:240,244` | `worker/server.rs:756` |
| `Summary` (1727) | `page/controller.rs:515` | `worker/server.rs:763` |
| `Found` (1728) | `page/commands.rs:214` | `worker/server.rs:766` |
| `SelectionStats` (1729) | `page/commands.rs:745` | `worker/server.rs:768` |
| `CanEdit` (1730) | `page/commands.rs:364` | `worker/server.rs:771` |
| `Review` (1731) | `page/review.rs:228` | `worker/server.rs:1360` |
| `ReviewMarkers` (1732) | `page/grid/mod.rs:2986` | `worker/server.rs:1386` |
| `Clipboard` (1733) | `page/controller.rs:1710,1844,1857` | `worker/server.rs:805` |
| `PastePreview` (1734) | `page/controller.rs:2043` | `worker/server.rs:1818` |
| `Pong` (1736) | `page/port.rs:122` | `worker/server.rs:820` |
| `Done` (1741) | `page/controller.rs:1179,1254,1290,1366,2262,4102,4129,4441,4476` | `worker/server.rs:912,918,1033,1039,2021,3442,3514` |
| `FitContents` (1748) | `page/controller.rs:2703` | `worker/server.rs:2054` |
| `Subscribed` (1752) | `page/port.rs:202` | `worker/server.rs:1152,1179,1187,1221,1225,1234` |
| `Geometry` (1753) | `page/grid/mod.rs:204` | `worker/server.rs:1329` |
| `UsedArea` (1754) | `page/grid/mod.rs:345` | `worker/server.rs:1330` |
| `ResolvedReference` (1755) | `page/controller.rs:2126,2161,3403,3411` | `worker/server.rs:1358` |
| `Charts` (1756) | `page/charts.rs:78,147`, `page/grid/mod.rs:356` | `worker/server.rs:1428` |
| `Shapes` (1757) | `page/grid/mod.rs:366` | `worker/server.rs:1429` |
| `Sparklines` (1758) | `page/grid/mod.rs:376` | `worker/server.rs:1430` |
| `Controls` (1759) | `page/grid/mod.rs:386` | `worker/server.rs:1431` |
| `PivotTables` (1760) | `page/pivots.rs:34` | `worker/server.rs:1426` |
| `PivotSourceFields` (1764) | `page/pivots.rs:87` | `worker/server.rs:1415` |
| `Validation` (1765) | `page/grid/mod.rs:3125` | `worker/server.rs:1539` |
| `ValidationInvalid` (1766) | `page/review.rs:60` | `worker/server.rs:1526` |
| `Atlas` (1767) | `page/atlas.rs:69` | `worker/server.rs:1442` |
| `AtlasRefusals` (1768) | `page/atlas.rs:136` | `worker/server.rs:1434` |
| `Outline` (1769) | `page/controller.rs:3509`, `page/tree.rs:236` | `worker/server.rs:1464` |
| `Notebook` (1770) | `page/notebook.rs:185` | `worker/server.rs:1475` |
| `BlockDetail` (1771) | `page/app.rs:284`, `page/notebook.rs:282` | `worker/server.rs:1482` |
| `Keys` (1772) | `page/tree.rs:302` | `worker/server.rs:1486` |
| `Canvas` (1773) | `page/canvas.rs:183` | `worker/server.rs:1499` |
| `Ledger` (1774) | `page/ledger.rs:191` | `worker/server.rs:1512` |
| `SavePending` (1775) | `page/controller.rs:4436,4469` | `worker/server.rs:1057,1071,1111` |
| `SaveImage` (1779) | `page/controller.rs:4449` | `worker/server.rs:1105` |

### DNA Calc Event (7)

Declarations at `model/projection.rs:1976`.

| Message | Page or client | Worker or desktop |
|---|---|---|
| `WorkbookEventFailed` (1977) | `page/controller.rs:734,931,947` | `worker/server.rs:3682,4014` |
| `SaveReady` (1984) | `page/controller.rs:752,930,946` | `worker/server.rs:404,3681` |
| `TileAdmission` (1988) | `page/controller.rs:925,942` | `worker/publication.rs:286`, `worker/server.rs:3676` |
| `Tiles` (1989) | `page/controller.rs:926,943`, `page/port.rs:369` | `desktop/app.rs:267`, `worker/publication.rs:287`, `worker/server.rs:3677`, `worker/transport.rs:39,41`, `worker/worker.rs:37` |
| `TileCompletion` (1990) | `page/controller.rs:927,944` | `worker/publication.rs:288`, `worker/server.rs:3678` |
| `ProjectionFailed` (1991) | `page/controller.rs:928,950,974` | `worker/server.rs:3679,4117,4143,4157,4167` |
| `Calculation` (1995) | `page/controller.rs:776,929,945` | `worker/server.rs:1766,3661,3680,4197` |

### DNA Calc FromServer (4)

Declarations at `model/projection.rs:2008`.

| Message | Page or client | Worker or desktop |
|---|---|---|
| `Started` (2010) | `page/port.rs:382` | `desktop/app.rs:286,355`, `worker/worker.rs:80` |
| `Reply` (2012) | `page/port.rs:398` | `desktop/app.rs:419,489`, `worker/server.rs:5129`, `worker/transport.rs:91` |
| `Failed` (2017) | `page/port.rs:405,415` | `desktop/app.rs:271,429,493`, `worker/server.rs:5130,5138`, `worker/transport.rs:95`, `worker/worker.rs:47,55,72,87,163` |
| `Event` (2021) | `page/port.rs:397` | `desktop/app.rs:275`, `worker/transport.rs:42,44`, `worker/worker.rs:60` |

### Formula Explorer Request (32)

Declarations at `one/port.rs:76`.

| Message | Page or client | Worker or desktop |
|---|---|---|
| `SetFormula` (78) | `one/port.rs:537` `one/port.rs:761` (test) | — |
| `SetFormulaCell` (82) | `one/port.rs:541` `one/port.rs:754` (test) | — |
| `SetInput` (86) | `one/port.rs:547` `one/port.rs:766` (test) | — |
| `SetEnvironment` (94) | `one/port.rs:563` `one/port.rs:896` (test) | — |
| `LoadVba` (100) | `one/port.rs:573,583`, `one/surface.rs:246` | `one-desktop/host.rs:735,744` |
| `Functions` (105) | `one/port.rs:594` `one/port.rs:854` (test) | — |
| `FunctionDocs` (108) | `one/port.rs:589`, `one/surface.rs:291` |  `one-desktop/host.rs:1485` (test) |
| `Calculate` (110) | `one/port.rs:712` `one/port.rs:776` (test) | — |
| `Evaluate` (112) | `one/port.rs:601`, `one/surface.rs:306` | `one-desktop/host.rs:768,794`, `one-desktop/measure.rs:55` |
| `UnloadVba` (114) | `one/port.rs:597`, `one/surface.rs:264` | `one-desktop/host.rs:762` |
| `EvaluatePart` (117) | `one/port.rs:604`, `one/surface.rs:164` | — |
| `Trace` (123) | `one/port.rs:612`, `one/surface.rs:221` | — |
| `New` (127) | `one/port.rs:613`, `one/surface.rs:608` | `one-desktop/host.rs:705` |
| `Open` (129) | `one/port.rs:620`, `one/surface.rs:610` | `one-desktop/host.rs:448,705,707,710` |
| `EnableMacros` (139) | `one/port.rs:684`, `one/surface.rs:1414` | `one-desktop/host.rs:673` |
| `UnblockMacros` (142) | `one/port.rs:684,685`, `one/surface.rs:1426` | `one-desktop/host.rs:673` |
| `MacroPolicy` (144) | `one/port.rs:679`, `one/surface.rs:368` | — |
| `SetMacroSecurity` (147) | `one/port.rs:680`, `one/surface.rs:1456` | `one-desktop/host.rs:690` |
| `OpenNative` (151) | `one/port.rs:700`, `one/surface.rs:618` | `one-desktop/host.rs:641` |
| `Save` (153) | `one/port.rs:652`, `one/surface.rs:715` | `one-desktop/host.rs:485,701,703` |
| `SaveNative` (156) | `one/port.rs:701`, `one/surface.rs:713,735` | `one-desktop/host.rs:699` |
| `SaveAsNative` (158) | `one/port.rs:702`, `one/surface.rs:718,733` | `one-desktop/host.rs:700` |
| `Recent` (161) | `one/port.rs:704`, `one/surface.rs:357` | `one-desktop/host.rs:650`, `one-desktop/measure.rs:102,142` |
| `OpenRecent` (163) | `one/port.rs:703`, `one/surface.rs:622` | `one-desktop/host.rs:646` |
| `SessionState` (168) | `one/port.rs:705`, `one/surface.rs:394` | `one-desktop/host.rs:651` |
| `OpenInDnaCalc` (173) | `one/port.rs:706`, `one/surface.rs:745` | `one-desktop/host.rs:656` |
| `LoadAddinNative` (176) | `one/port.rs:707`, `one/surface.rs:515` | `one-desktop/host.rs:721` |
| `LoadAddin` (180) | `one/port.rs:707`, `one/surface.rs:522,532` | `one-desktop/app.rs:1061`, `one-desktop/host.rs:726`, `one-desktop/measure.rs:78` |
| `UnloadAddin` (185) | `one/port.rs:707`, `one/surface.rs:576` | `one-desktop/host.rs:730` |
| `Addins` (189) | `one/port.rs:710`, `one/surface.rs:582` | `one-desktop/app.rs:852`, `one-desktop/host.rs:731` |
| `StopAddinProcess` (193) | `one/port.rs:711`, `one/surface.rs:557` | `one-desktop/app.rs:732,821` |
| `RestartAddinProcess` (197) | `one/port.rs:711`, `one/surface.rs:565` | `one-desktop/app.rs:736` |

### Formula Explorer Reply (17)

Declarations at `one/port.rs:309`.

| Message | Page or client | Worker or desktop |
|---|---|---|
| `Accepted` (311) | `one/port.rs:539,569,599,705`, `one/surface.rs:747` | `one-desktop/host.rs:654,666,766` |
| `Cell` (313) | `one/port.rs:542,557` | — |
| `Calculated` (316) | `one/port.rs:714` | — |
| `Functions` (318) | `one/port.rs:577,585,594`, `one/surface.rs:252` `one/port.rs:855` (test) | `one-desktop/host.rs:760` `one-desktop/host.rs:1601` (test) |
| `FunctionDocs` (322) | `one/port.rs:589`, `one/surface.rs:293` |  `one-desktop/host.rs:1485` (test) |
| `Refused` (323) | `one/bridge.rs:113,128`, `one/port.rs:528`, `one/surface.rs:172,253,510,604,706,748` | `one-desktop/app.rs:738,742,1071`, `one-desktop/host.rs:357,425,428,498,509,642,647,657,667,722,747,750`, `one-desktop/measure.rs:90,125`, `one-desktop/surrogate.rs:72,458,748,753,760,766,768,772` |
| `Shown` (327) | `one/port.rs:602`, `one/surface.rs:308` | `one-desktop/measure.rs:60` |
| `Part` (329) | `one/port.rs:609`, `one/surface.rs:171` | — |
| `Trace` (331) | `one/port.rs:612`, `one/surface.rs:225` | — |
| `Opened` (333) | `one/port.rs:482`, `one/surface.rs:591` | `one-desktop/host.rs:340,350,453,476,675,683,715` |
| `NotASession` (335) | `one/port.rs:645`, `one/surface.rs:596` | `one-desktop/host.rs:473` |
| `File` (340) | `one/port.rs:656`, `one/surface.rs:686` | `one-desktop/host.rs:486` |
| `Saved` (346) | `one/surface.rs:697` | `one-desktop/host.rs:495,506,530` |
| `Cancelled` (350) | `one/surface.rs:509,602,705` | `one-desktop/host.rs:508,643,723` |
| `Addins` (352) | `one/port.rs:710`, `one/surface.rs:499,584` | `one-desktop/app.rs:734,737,1070`, `one-desktop/host.rs:403,439,731` |
| `Recent` (354) | `one/port.rs:704`, `one/surface.rs:359` | `one-desktop/host.rs:650`, `one-desktop/measure.rs:143` |
| `MacroPolicy` (356) | `one/port.rs:679,681`, `one/surface.rs:370,603` | `one-desktop/host.rs:692` |
