# Office.js (Excel JavaScript API) Inventory and COM Descriptor Map

Status: inventory only (dc-17spk.4, building on dc-1nbp.30). **Zero Office.js / `Excel.run` / `RequestContext` / `CustomFunctions` runtime or projection code exists in `apps/` or `crates/` today** (confirming finding C28 of [14-harvest.md](review-2026-11/pro/harvest/14-harvest.md)). All implemented behaviour listed below is currently exposed only through the COM object model descriptor ([`crates/calc-object-model`](../../crates/calc-object-model/src/descriptor.rs)) and the host profiles (`ValuesOnly` in [`crates/calc-object-model`](../../crates/calc-object-model/src/profile/values_only.rs), `WorkbookAutomation` in [`crates/calc-host`](../../crates/calc-host/src/automation.rs)), plus session events in [`crates/calc-host/src/events.rs`](../../crates/calc-host/src/events.rs).

Sources used:
- **Office.js public API surface**: Microsoft's TypeScript definitions (`@types/office-js` `declare namespace Excel`, `OfficeExtension`, and `@types/custom-functions-runtime`).
- **COM descriptor**: [`crates/calc-object-model/descriptor/`](../../crates/calc-object-model/descriptor/) (`excel.json` Excel 1.9 `16.0.20527.20150`, `office.json` Office 2.8 `16.0.20527.20112`, `addin-designer.json` 1.0, `accessibility.json` 1.1).
- **Current host implementation**: [`OBJECT_MODEL.md`](OBJECT_MODEL.md), [`crates/calc-object-model/src/profile/values_only.rs`](../../crates/calc-object-model/src/profile/values_only.rs), [`crates/calc-object-model/src/profile/worksheet_function.rs`](../../crates/calc-object-model/src/profile/worksheet_function.rs), [`crates/calc-host/src/automation.rs`](../../crates/calc-host/src/automation.rs) (and `navigation.rs`, `mutation.rs`, `formula.rs`, `report.rs`, `report_draw.rs`, `value.rs`), and [`crates/calc-host/src/events.rs`](../../crates/calc-host/src/events.rs).

---

## 1. Executive Summary and Counts

### 1.1 Surface Totals

| Surface | Types / Interfaces | Unique Members | Notes |
|---|---:|---:|---|
| **Office.js `Excel` namespace** | 193 classes (+ 721 interfaces, 179 enums, 10 types, 8 functions) | **2,764** (3,300 declarations w/ overloads) | Includes 57 `on*` event properties (`EventHandlers<T>`) and 366 worksheet functions on `Excel.Functions`. |
| — Proxy / batching boilerplate | (on `ClientObject` subclasses) | 741 | `context` (192), `toJSON` (192), `load` (187), `set` (112), `items` (52), `track`/`untrack` (6). |
| — **Office.js domain members** | 193 classes | **2,023** | 958 methods + 2 static methods, 448 readonly properties, 558 read-write properties, 57 events. |
| **COM Descriptor (`descriptor/*.json`)** | 63 types across 4 libraries (Excel 40, Office 21, AddInDesigner 1, Accessibility 1) | **3,545** (57 complete advertised layouts) | Excel 1.9: 2,916 (36 layouts); Office 2.8: 603 (19 layouts); Accessibility 1.1: 21; AddInDesigner 1.0: 5. |
| **`calc-object-model` (`ValuesOnly`)** | 7 interfaces (`_Application`, `Workbooks`, `_Workbook`, `_Worksheet`, `Window`, `Range`, `WorksheetFunction`) | **464** registered handlers | 420 `WorksheetFunction` + 44 others (13 of the 44 are explicit `needs_workbook` refusals). |
| **`calc-host` (`WorkbookAutomation`)** | 26 interfaces (core workbook + report objects + `ValuesOnly` fallback) | **626** registered handlers | 420 `WorksheetFunction` + 206 workbook/report handlers (`automation.rs`, `navigation.rs`, `mutation.rs`, `formula.rs`, `report.rs`, plus `ValuesOnly` fallback). |
| **Office.js projection in `apps/` / `crates/`** | 0 | **0** | Not yet implemented (owned by `dc-17spk` / `dc-1nbp.30`). |

### 1.2 Core Families Coverage Summary (Sections 4–12)

Sections 4–12 inventory the core classes named in `dc-17spk.4` plus `RequestContext`/`Runtime`/`Application`/`Window`/`Shape`/`Functions`/`Events` (**52 classes**, **1,201** domain members, plus **10** non-core events = **1,211** unique domain members across **56** classes). Status tags used in the tables below:

- **Descriptor status**:
  - `COM: <Iface>.<Member> (<kind>)` — exact member exists in `crates/calc-object-model/descriptor/*.json`.
  - `COM-absent (<Excel COM member>)` — Excel's COM type library defines an equivalent object/member (e.g. `Font`, `Interior`, `Borders`, `PageSetup`, `Validation`, `FormatCondition`, `PivotTable`, `Pane`), but it is not yet listed in `descriptor/*.interfaces.txt`.
  - `None (Office.js-only)` — no COM equivalent (`RequestContext`, `load`/`toJSON`, `*OrNullObject`, `TrackedObjects`, `Setting`, `NamedSheetView`, `valuesAsJson`, `suspendApiCalculationUntilNextSync`, etc.).
- **`calc-host` status**:
  - `Impl` — implemented in `WorkbookAutomation` (`calc-host`).
  - `Impl (VO)` — implemented in `ValuesOnly` (`calc-object-model`) and inherited by `WorkbookAutomation`.
  - `Partial` — COM handler exists, with noted Office.js semantic/argument gaps (e.g., `Range.Value2`/`Formula`/`Text` limits, `Names` global-only, `ListObjects.Add` header-row requirement).
  - `Event-src` — session event exists in [`crates/calc-host/src/events.rs`](../../crates/calc-host/src/events.rs) (`WorkbookEvent` / `WorkbookEventObserver`), not exposed to Office.js yet.
  - `No` — not implemented in `calc-host` today (COM returns uniform 1004 refusal or interface is absent from descriptor).

| Core Family (Office.js Classes) | Classes | Domain Members | `calc-host` COM-Surface Overlap (Impl / Partial) | Notes |
|---|---:|---:|---:|---|
| `RequestContext`, `Runtime`, `Application`, `IterativeCalculation`, `WindowCollection`, `Window` | 6 | 81 | 5 | `RequestContext.{workbook, application}`, `Runtime.enableEvents`, `Application.{calculate, union}` |
| `Workbook`, `WorkbookProtection` | 2 | 52 | 5 | `Workbook.{application, functions, names (partial), worksheets, name}` |
| `WorksheetCollection`, `Worksheet`, `WorksheetProtection`, `WorksheetFreezePanes` | 4 | 107 | 15 (+ 4 `Event-src`) | `WorksheetCollection` (5) + `Worksheet` (10); `Worksheet.{onChanged, onCalculated}` & `WorksheetCollection.{onChanged, onCalculated}` have `Event-src` |
| `Range`, `RangeAreas`, `RangeView*`, `RangeFormat`, `RangeFill`, `RangeBorder*`, `RangeFont`, `FormatProtection` | 10 | 204 | 23 | `Range` (20) + `RangeAreas` (3); formatting/protection/view have 0 handlers |
| `TableCollection`, `TableScopedCollection`, `Table`, `TableColumn*`, `TableRow*`, `TableSort`, `Filter`, `AutoFilter` | 10 | 101 | 23 | `TableCollection` (5) + `Table` (10) + `TableColumn*` (5) + `TableRowCollection` (3); `ListRow`/sort/filter have 0 handlers |
| `ChartCollection`, `Chart`, `ChartTitle`, `ChartSeriesCollection`, `ChartSeries`, `ChartAxes`, `ChartAxis`, `ChartLegend` | 8 | 171 | 26 | `ChartCollection` (5) + `Chart` (12) + `ChartTitle` (2) + `ChartSeriesCollection` (3) + `ChartSeries` (4); `Axes`/`Axis`/`Legend` have 0 handlers |
| `NamedItemCollection`, `NamedItem`, `NamedItemArrayValues` | 3 | 24 | 5 | `NamedItemCollection` (2 partial) + `NamedItem` (3 partial); global names only (`dc-hsa5.6`) |
| `ShapeCollection`, `Shape`, `Line`, `ShapeFill`, `ShapeLineFormat`, `TextFrame`, `TextRange` | 7 | 93 | 34 | `ShapeCollection` (6) + `Shape` (14) + `Line` (8) + `ShapeFill` (2) + `ShapeLineFormat` (2) + `TextFrame` (1) + `TextRange` (1) |
| `Functions`, `FunctionResult<T>` | 2 | 368 | 366 (`calc-catalogue`) | All 366 `Excel.Functions` worksheet names exist in `calc-catalogue` (302 via COM `WorksheetFunction`, 64 via `calc-func` directly) |
| **Non-core events** (`Binding`, `SettingCollection`, `CommentCollection`, `LinkedEntityDataDomainCollection`) | (4) | 10 | 0 | All 57 events (47 inside the classes above + 10 here) are inventoried in §12 |
| **Total inventoried** | **52 (+4)** | **1,211** | **502** (136 OM + 366 functions) | Plus 4 `Event-src` in `calc-host/src/events.rs` |

---

## 2. Requirement Sets (`ExcelApi 1.1` – `1.21`, Desktop, Online)

Every class and member in `@types/office-js` declares an `[Api set: ...]` tag. Counting each class and each unique `(Class, member)` by its **earliest** introduced requirement set across all 193 classes (**2,764** unique members, including proxy boilerplate):

| Requirement Set | Classes Introduced | Unique Members Introduced | Key Objects / Capabilities Added | Current `calc-host` Overlap |
|---|---:|---:|---|---|
| `(none)` | 1 | 3 | `RequestContext` (`workbook`, `application`, `constructor`) | 0 (no JS batch context) |
| **ExcelApi 1.1** | 45 | 388 | `Application`, `Workbook`, `Worksheet`, `WorksheetCollection`, `Range`, `RangeFormat`/`Fill`/`Border`/`Font`, `Table*`, `Chart*`, `NamedItem*`, `Binding*` | Primary overlap: core Workbook/Worksheet/Range/Table/Chart/Name members |
| **ExcelApi 1.2** | 7 | 443 | `Functions` (368), `FunctionResult` (5), `WorksheetProtection`, `FormatProtection`, `RangeSort`, `TableSort`, `Filter`, `Workbook.onSelectionChanged` | `Functions` (366 worksheet functions match `calc-catalogue`) |
| **ExcelApi 1.3** | 4 | 47 | `RangeView`, `RangeViewCollection`, `PivotTable`, `PivotTableCollection`, `Table` style flags | Minimal (`Table` style flags unmodelled) |
| **ExcelApi 1.4** | 2 | 47 | `Setting`, `SettingCollection`, `*OrNullObject`, `*getCount()`, `Worksheet.names`, `NamedItemCollection.add`, `NamedItem.scope/delete` | 0 (`Names` is workbook-global only; no `Setting`) |
| **ExcelApi 1.5** | 4 | 39 | `Runtime`, `CustomXmlPart*`, `Worksheet.getNext`/`getPrevious`/`getFirst`/`getLast` | 0 |
| **ExcelApi 1.6** | 18 | 160 | `ConditionalFormat*` (18 classes), `Application.suspendApiCalculationUntilNextSync`, `Worksheet.calculate`, `Range.calculate` | `Worksheet.calculate`, `Range.calculate` (COM `Calculate`) |
| **ExcelApi 1.7** | 17 | 257 | `WorkbookProtection`, `WorksheetFreezePanes`, `NamedItemArrayValues`, `DocumentProperties`, `Style*`, `ChartTrendline*`, `ChartDataLabel`, `Worksheet` events (`onActivated`, `onChanged`, `onSelectionChanged`), `Table.onChanged` | `NamedItem.formula` (`Name.RefersTo`), `Worksheet.onChanged` (`Event-src`), `Chart.chartType` (`_Chart.ChartType`), `ChartSeries.setValues`/`setXAxisValues` |
| **ExcelApi 1.8** | 19 | 258 | `DataValidation`, `PivotLayout`, `PivotHierarchy*`, `PivotField*`, `ChartPlotArea*`, `Runtime.enableEvents`, `Worksheet.onCalculated`, `Chart` events | `Runtime.enableEvents` (`_Application.EnableEvents`), `Chart.plotBy`/`style`, `Worksheet.onCalculated` (`Event-src`) |
| **ExcelApi 1.9** | 29 | 451 | `IterativeCalculation`, `RangeAreas`, `ShapeCollection`, `Shape`, `Line`, `ShapeFill`, `ShapeLineFormat`, `TextFrame`, `TextRange`, `PageLayout`, `HeaderFooter*`, `AutoFilter`, `Range.find`/`flashFill`/`removeDuplicates`/`copyFrom`, `TableCollection.onAdded`/`onDeleted` | `ShapeCollection`/`Shape`/`Line`/`ShapeFill`/`ShapeLineFormat`/`TextFrame`/`TextRange` (COM `Shapes`/`Shape`/`TextFrame2`/`FillFormat`/`LineFormat`/`ConnectorFormat`) |
| **ExcelApi 1.10** | 16 | 187 | `Comment*` (4), `Slicer*` (4), `*Style*` (8), `Range.left`/`top`/`width`/`height`/`group`, `Worksheet.onSingleClicked` | 0 |
| **ExcelApi 1.11** | 2 | 28 | `CultureInfo`, `NumberFormatInfo`, `Application.decimalSeparator`/`thousandsSeparator`, `Workbook.save`/`close`, `Range.moveTo` | 0 |
| **ExcelApi 1.12** | 6 | 71 | `WorkbookRangeAreas`, `RangeAreasCollection`, `DatetimeFormatInfo`, `PivotTableScopedCollection`, `WorksheetCustomProperty*`, `Range.hasSpill`/`getSpillParent`/`getSpillingToRange` | 0 |
| **ExcelApi 1.13** | 0 | 17 | `Workbook.onActivated`, `Worksheet.onFormulaChanged`, `Table.resize`, `Range.getDirectDependents`/`getExtendedRange`/`getRangeEdge` | 0 |
| **ExcelApi 1.14** | 4 | 47 | `Query`, `QueryCollection`, `ChartDataTable`, `ChartDataTableFormat`, `Worksheet.tabId`, `Worksheet.onProtectionChanged` | 0 |
| **ExcelApi 1.15** | 0 | 7 | `Range.getDependents`, `Shape.displayName`, `ChartSeries.getDimensionDataSource*` | 0 |
| **ExcelApi 1.16** | 0 | 16 | `*valuesAsJson` / `*valuesAsJsonLocal` (rich cell value JSON protocol) | 0 |
| **ExcelApi 1.17** | 0 | 16 | `Worksheet.onMoved`/`onNameChanged`/`onVisibilityChanged` | 0 |
| **ExcelApi 1.18** | 2 | 26 | `Note`, `NoteCollection`, `Range.control` (`CheckboxCellControl`), `Range.clearOrResetContents` | 0 |
| **ExcelApi 1.19** | 5 | 60 | `LinkedEntityDataDomain*`, `ChartDataLabelAnchor`, `ChartLeaderLines*`, `Worksheet.showDataTypeIcons` | 0 |
| **ExcelApi 1.20** | 1 | 7 | `CustomFunctionManager` | 0 |
| **ExcelApi 1.21** | 0 | 2 | `LinkedEntityDataDomainCollection.onLinkedEntityCellValueLoaded`, `NamedItemCollection` formula overload | 0 |
| **ExcelApiDesktop 1.1** | 5 | 120 | `Window`, `WindowCollection`, `Pane`, `PaneCollection`, `HeaderFooterPicture`, `Application.activeWindow`/`windows`/`union`/`checkSpelling`/`enterEditingMode`, `Range.formulaArray` | `Application.union` (`_Application.Union`), `Window` (COM `Window` has 3 VO handlers) |
| **ExcelApiOnline 1.1** | 6 | 67 | `AllowEditRange*`, `LinkedWorkbook*`, `NamedSheetView*`, `WorksheetProtection` pause/password, `TableRowCollection.deleteRows*` | 0 |
| **Total** | **193** | **2,764** | | |

---

## 3. Full `Excel` Namespace Overview (All 193 Classes)

Every class in `declare namespace Excel` grouped by domain (summing to **193 classes**, **2,764 unique members**, **57 events**):

| Domain | Classes | Unique Members | Events | Detailed in Section | Current `calc-host` Coverage |
|---|---:|---:|---:|---|---|
| Runtime, Context, Application & Functions | 10 | 434 | 0 | §4, §11 | `Application` (4 COM), `Functions` (366 in `calc-catalogue`) |
| Workbook, Window, Settings & Document Properties | 20 | 274 | 4 | §4, §5, §12 | `Workbook` (4 COM), `Window` (3 VO) |
| Worksheet, Protection, FreezePanes, Views & PageLayout | 16 | 266 | 31 | §6, §12 | `WorksheetCollection` (4 COM), `Worksheet` (8 COM), `WorksheetChange`/`Calculate` (`Event-src`) |
| Range, RangeAreas, RangeView, Format, Sort, Validation, CF | 33 | 449 | 0 | §7 | `Range` (18 COM), `RangeAreas` (3 COM) |
| Table, Columns, Rows, Filter, AutoFilter & TableStyle | 12 | 156 | 5 | §8 | `TableCollection` (4), `Table` (8), `TableColumn*` (4), `TableRow*` (3) |
| Chart, Series, Axes, Legend, Title, DataLabels, Trendlines | 48 | 560 | 6 | §9 | `ChartCollection` (4), `Chart` (7), `ChartTitle` (2), `ChartSeries*` (6) |
| NamedItem & NamedItemArrayValues | 3 | 35 | 0 | §10 | `NamedItemCollection` (3), `NamedItem` (4) |
| Shape, GeometricShape, Image, Line, Fill, TextFrame | 12 | 167 | 2 | §10a | `ShapeCollection` (7), `Shape` (8), `Line` (6), `ShapeFill` (1), `ShapeLineFormat` (2), `TextRange` (1) |
| PivotTable, Slicer, Timeline, Query, Connections, LinkedEntity | 31 | 323 | 4 | (Unmodelled) | 0 |
| Comments, CommentReplies, Notes & Bindings | 8 | 100 | 5 | §12 (events) | 0 |
| **Total** | **193** | **2,764** | **57** | | |

---

## 4. Runtime, `RequestContext`, `Application`, and `Window`

Every `OfficeExtension.ClientObject` subclass carries 4 proxy/batching members (`context`, `load`, `set`, `toJSON`; collections also carry `items`; `Range`/`RangeAreas` also carry `track`/`untrack`; `Functions` and `RequestContext` omit `set`/`load`). Boilerplate members have **no COM counterpart** (`None (Office.js-only)`) and belong to the proxy/batching layer (`Excel.run`).

### 4.1 `Excel.run`, `RequestContext`, `Runtime`, and `IterativeCalculation`

- **`Excel.run`** (5 TypeScript overloads, `ExcelApi 1.1` / `1.2` / `1.9` options): opens a batch `RequestContext`, executes `batch(context)`, flushes queued operations via `context.sync()`, and returns a `Promise<T>`. No COM counterpart (`None`).
- **`OfficeExtension.TrackedObjects`** (`context.trackedObjects.add/remove`, `ExcelApi 1.1`): extends remote object leases across `context.sync()` boundaries. Maps semantically to session `ReferenceHandles` / `ObjectModel::released` lease retention in `calc-host/src/automation.rs`, though COM uses `IUnknown::AddRef`/`Release`.

| Class.Member | Kind | Req Set | COM Descriptor Member | `calc-host` Today | Notes |
|---|---|---|---|---|---|
| `RequestContext.workbook` | prop(ro) | (core) | `_Application.ActiveWorkbook (Get)` | `Impl` | Returns active `Workbook` |
| `RequestContext.application` | prop(ro) | (core) | `_Application.Application (Get)` | `Impl (VO)` | Returns `Application` |
| `RequestContext.runtime` | prop(ro) | 1.5 | `None (Office.js-only)` | `No` | Holds `Runtime` |
| `RequestContext.constructor` | method | (core) | `None (Office.js-only)` | `No` | JS constructor |
| `Runtime.enableEvents` | prop(rw) | 1.8 | `_Application.EnableEvents (Get/Put)` | `Impl (VO)` | Shared `workbook_events_switch` in `calc-host/src/events.rs` |
| `IterativeCalculation.enabled` | prop(rw) | 1.9 | `_Application.Iteration (Get/Put)` | `No` | Not in `WorkbookAutomation` |
| `IterativeCalculation.maxChange` | prop(rw) | 1.9 | `_Application.MaxChange (Get/Put)` | `No` | Not in `WorkbookAutomation` |
| `IterativeCalculation.maxIteration` | prop(rw) | 1.9 | `_Application.MaxIterations (Get/Put)` | `No` | Not in `WorkbookAutomation` |

### 4.2 `Application` (16 domain members + 4 boilerplate = 20)

| Office.js Member | Kind | Req Set | COM Descriptor Member | `calc-host` Today | Notes |
|---|---|---|---|---|---|
| `activeWindow` | prop(ro) | Desktop 1.1 | `_Application.ActiveWindow (Get)` | `No` | COM `Window` only via `WM_GETOBJECT` |
| `cultureInfo` | prop(ro) | 1.11 | `None (Office.js-only)` | `No` | Uses locale profile internally (`calc-func::locale`) |
| `iterativeCalculation` | prop(ro) | 1.9 | `_Application.{Iteration,MaxChange,MaxIterations}` | `No` | Separate sub-object in Office.js |
| `windows` | prop(ro) | Desktop 1.1 | `_Application.Windows (Get)` | `No` | `Windows` in descriptor (0 handlers) |
| `calculationEngineVersion` | prop(ro) | 1.9 | `_Application.CalculationVersion (Get)` | `No` | — |
| `calculationMode` | prop(rw) | 1.1 | `_Application.Calculation (Get/Put)` | `No` | Session supports manual/auto internally |
| `calculationState` | prop(ro) | 1.9 | `_Application.CalculationState (Get)` | `No` | — |
| `decimalSeparator` | prop(ro) | 1.11 | `_Application.DecimalSeparator (Get)` | `No` | — |
| `thousandsSeparator` | prop(ro) | 1.11 | `_Application.ThousandsSeparator (Get)` | `No` | — |
| `useSystemSeparators` | prop(ro) | 1.11 | `_Application.UseSystemSeparators (Get)` | `No` | — |
| `calculate` | method | 1.1 | `_Application.Calculate` / `CalculateFull` / `CalculateFullRebuild` | `Impl` | COM splits 3 methods; OJS takes `CalculationType` (`Recalculate`, `Full`, `FullRebuild`) |
| `checkSpelling` | method | Desktop 1.1 | `_Application.CheckSpelling (Method)` | `No` | — |
| `enterEditingMode` | method | Desktop 1.1 | `None (Office.js-only)` | `No` | UI focus |
| `suspendApiCalculationUntilNextSync` | method | 1.6 | `None (Office.js-only)` | `No` | Batch-scoped recalc suppression |
| `suspendScreenUpdatingUntilNextSync` | method | 1.9 | `_Application.ScreenUpdating (Get/Put)` | `No` | Batch-scoped |
| `union` | method | Desktop 1.1 | `_Application.Union (Method)` | `Impl` | `automation.rs` (`Self::union`); OJS returns `RangeAreas` |

### 4.3 `WindowCollection` (5 domain + 4) & `Window` (52 domain + 4) — `ExcelApiDesktop 1.1`

- **`WindowCollection`**: `getCount` (`Windows.Count`), `getItemAt` (`Windows.Item`), `breakSideBySide` (`Windows.BreakSideBySide`), `compareCurrentSideBySideWith` (`Windows.CompareSideBySideWith`), `resetPositionsSideBySide` (`Windows.ResetPositionsSideBySide`). `calc-host` implements **0/5** (`Windows` has 0 handlers).
- **`Window`** (52 domain members): 44 map directly to COM `Window` (`ActiveCell`, `ActivePane`, `ActiveSheet`, `Panes`, `VisibleRange`, `AutoFilterDateGrouping`, `EnableResize`, `FreezePanes`, `Height`, `Index`, `Visible`, `Left`, `Caption`, `ScrollColumn`, `ScrollRow`, `DisplayFormulas`, `DisplayGridlines`, `DisplayHeadings`, `DisplayHorizontalScrollBar`, `DisplayOutline`, `DisplayRightToLeft`, `DisplayRuler`, `DisplayVerticalScrollBar`, `DisplayWhitespace`, `DisplayWorkbookTabs`, `DisplayZeros`, `Split*`, `TabRatio`, `Top`, `Type`, `UsableHeight`, `UsableWidth`, `View`, `Width`, `WindowNumber`, `WindowState`, `Zoom`, `Activate`, `ActivateNext`, `ActivatePrevious`, `Close`, `LargeScroll`, `NewWindow`, `PointsToScreenPixelsX/Y`, `ScrollIntoView`, `ScrollWorkbookTabs`, `SmallScroll`); `Pane`/`PaneCollection` (10 members) correspond to COM-absent `Pane`/`Panes`. `calc-host` implements **0** `Window` members beyond `ValuesOnly` (`Application`, `Parent`, `Hwnd`; `Window.Parent` overridden in `automation.rs` to return active `Workbook`).

---

## 5. `Workbook` and `WorkbookProtection`

### 5.1 `Workbook` (49 domain members + 4 boilerplate = 53)

| Office.js Member | Kind | Req Set | COM Descriptor Member | `calc-host` Today | Notes |
|---|---|---|---|---|---|
| `application` | prop(ro) | 1.1 | `_Workbook.Application (Get)` | `Impl (VO)` | Returns `Application` |
| `bindings` | prop(ro) | 1.1 | `None (Office.js-only)` | `No` | Office.js add-in bindings |
| `comments` | prop(ro) | 1.10 | `_Workbook.Comments (Get)` | `No` | Modern threaded comments collection |
| `customXmlParts` | prop(ro) | 1.5 | `_Workbook.CustomXMLParts (Get)` | `No` | `calc-doc` preserves OPC custom XML |
| `dataConnections` | prop(ro) | 1.7 | `_Workbook.Connections (Get)` | `No` | — |
| `functions` | prop(ro) | 1.2 | `_Application.WorksheetFunction (Get)` | `Impl (VO)` | On `Application` in COM, on `Workbook` in OJS |
| `linkedEntityDataDomains` | prop(ro) | 1.19 | `None (Office.js-only)` | `No` | Rich data types |
| `linkedWorkbooks` | prop(ro) | Online 1.1 | `None (Office.js-only)` | `No` | Web linked workbooks |
| `names` | prop(ro) | 1.1 | `_Workbook.Names (Get)` | `Partial` | Global names only; sheet-local names refuse (`dc-hsa5.6`) |
| `notes` | prop(ro) | 1.18 | `None (Office.js-only)` | `No` | Workbook notes collection |
| `pivotTableStyles` | prop(ro) | 1.10 | `COM-absent (Workbook.TableStyles / Pivot)` | `No` | — |
| `pivotTables` | prop(ro) | 1.3 | `_Workbook.PivotTables (Get)` | `No` | — |
| `properties` | prop(ro) | 1.7 | `_Workbook.BuiltinDocumentProperties` / `CustomDocumentProperties` | `No` | — |
| `protection` | prop(ro) | 1.7 | `_Workbook.ProtectStructure` / `ProtectWindows` / `Protect` / `Unprotect` | `No` | — |
| `queries` | prop(ro) | 1.14 | `_Workbook.Queries (Get)` | `No` | Power Query |
| `settings` | prop(ro) | 1.4 | `None (Office.js-only)` | `No` | Add-in persisted key/value settings |
| `slicerStyles` | prop(ro) | 1.10 | `None (Office.js-only)` | `No` | — |
| `slicers` | prop(ro) | 1.10 | `_Workbook.SlicerCaches (Get)` | `No` | — |
| `styles` | prop(ro) | 1.7 | `_Workbook.Styles (Get)` | `No` | — |
| `tableStyles` | prop(ro) | 1.10 | `_Workbook.TableStyles (Get)` | `No` | — |
| `tables` | prop(ro) | 1.1 | `None` (COM has `Worksheet.ListObjects`) | `No` | Workbook-scoped union of sheet `ListObjects` |
| `timelineStyles` | prop(ro) | 1.10 | `None (Office.js-only)` | `No` | — |
| `worksheets` | prop(ro) | 1.1 | `_Workbook.Worksheets (Get)` | `Impl` | `automation/navigation.rs` (`Self::sheets`) |
| `autoSave` | prop(ro) | 1.9 | `_Workbook.AutoSaveOn (Get)` | `No` | — |
| `calculationEngineVersion` | prop(ro) | 1.9 | `_Workbook.CalculationVersion (Get)` | `No` | — |
| `chartDataPointTrack` | prop(rw) | 1.9 | `_Workbook.ChartDataPointTrack (Get/Put)` | `No` | — |
| `isDirty` | prop(rw) | 1.9 | `_Workbook.Saved (Get/Put)` (inverted) | `No` | — |
| `name` | prop(ro) | 1.7 | `_Workbook.Name (Get)` | `Impl` | `automation.rs` (`Self::workbook_name`) |
| `previouslySaved` | prop(ro) | 1.9 | `None (Office.js-only)` (`_Workbook.Path != ""`) | `No` | — |
| `readOnly` | prop(ro) | 1.8 | `_Workbook.ReadOnly (Get)` | `No` | — |
| `usePrecisionAsDisplayed` | prop(rw) | 1.9 | `_Workbook.PrecisionAsDisplayed (Get/Put)` | `No` | — |
| `close` | method | 1.11 | `_Workbook.Close (Method)` | `No` | — |
| `focus` | method | Desktop 1.1 | `_Workbook.Activate (Method)` | `No` | — |
| `getActiveCell` | method | 1.7 | `_Application.ActiveCell (Get)` | `No` | — |
| `getActiveChart` / `*OrNullObject` | method | 1.9 | `_Workbook.ActiveChart (Get)` | `No` | 2 members (`*OrNullObject` is OJS-only pattern) |
| `getActiveShape` / `*OrNullObject` | method | 1.19 | `None (Office.js-only)` | `No` | 2 members |
| `getActiveSlicer` / `*OrNullObject` | method | 1.10 | `_Workbook.ActiveSlicer (Get)` | `No` | 2 members |
| `getIsActiveCollabSession` | method | 1.9 | `None (Office.js-only)` | `No` | Co-authoring |
| `getLinkedEntityCellValue` | method | 1.16 | `None (Office.js-only)` | `No` | — |
| `getSelectedRange` | method | 1.1 | `_Application.Selection (Get)` | `No` | — |
| `getSelectedRanges` | method | 1.9 | `_Application.Selection (Get)` | `No` | Returns `RangeAreas` |
| `insertWorksheetsFromBase64` | method | 1.13 | `None (Office.js-only)` | `No` | — |
| `save` | method | 1.11 | `_Workbook.Save (Method)` | `No` | — |
| `onActivated` | event | 1.13 | `WorkbookEvents.Activate` | `No` | See §12 |
| `onAutoSaveSettingChanged` | event | 1.9 | `None (Office.js-only)` | `No` | See §12 |
| `onSelectionChanged` | event | 1.2 | `WorkbookEvents.SheetSelectionChange` | `No` | See §12 |

- **`WorkbookProtection`** (`ExcelApi 1.7`, 3 domain members: `protected`, `protect`, `unprotect`): maps to `_Workbook.{ProtectStructure, Protect, Unprotect}`; `calc-host` implements **0/3**.

---

## 6. `WorksheetCollection` and `Worksheet`

### 6.1 `WorksheetCollection` (24 domain members + 4 boilerplate = 28)

| Office.js Member | Kind | Req Set | COM Descriptor Member | `calc-host` Today | Notes |
|---|---|---|---|---|---|
| `add` | method | 1.1 | `Sheets.Add (Method)` / `Add2` | `No` | — |
| `getActiveWorksheet` | method | 1.1 | `_Workbook.ActiveSheet (Get)` | `Impl` | `automation.rs` (`Self::get_active_sheet`) |
| `getCount` | method | 1.4 | `Sheets.Count (Get)` | `Impl` | `automation/navigation.rs` (`Self::sheets_count`) |
| `getFirst` / `getLast` | method | 1.5 | `Sheets.Item(1)` / `Sheets.Item(Count)` | `Impl` (via `Item`) | 2 members; `Sheets.Item` is implemented |
| `getItem` | method | 1.1 | `Sheets.Item (Get)` | `Impl` | By name (case-insensitive) or index |
| `getItemOrNullObject` | method | 1.4 | `None (Office.js-only)` | `No` | Null-object wrapper around `Sheets.Item` |
| `onActivated`, `onAdded`, `onCalculated`, `onChanged`, `onColumnSorted`, `onDeactivated`, `onDeleted`, `onFormatChanged`, `onFormulaChanged`, `onMoved`, `onNameChanged`, `onProtectionChanged`, `onRowHiddenChanged`, `onRowSorted`, `onSelectionChanged`, `onSingleClicked`, `onVisibilityChanged` | event (×17) | 1.7–1.17 | `WorkbookEvents.Sheet*` (6 match: `SheetActivate`, `NewSheet`, `SheetCalculate`, `SheetChange`, `SheetDeactivate`, `SheetSelectionChange`; `SheetBeforeDelete` is pre-delete) | `Event-src` (2: `WorksheetChange`, `WorksheetCalculate`) | See §12 for full 17-event breakdown |

### 6.2 `Worksheet` (63 domain members + 4 boilerplate = 67)

| Office.js Member | Kind | Req Set | COM Descriptor Member | `calc-host` Today | Notes |
|---|---|---|---|---|---|
| `autoFilter` | prop(ro) | 1.9 | `_Worksheet.AutoFilter (Get)` | `No` | — |
| `charts` | prop(ro) | 1.1 | `_Worksheet.ChartObjects (Method)` | `Impl` | `automation/report.rs` (`Self::sheet_chart_objects`) |
| `comments` | prop(ro) | 1.10 | `_Worksheet.CommentsThreaded (Get)` | `No` | — |
| `customProperties` | prop(ro) | 1.12 | `_Worksheet.CustomProperties (Get)` | `No` | — |
| `freezePanes` | prop(ro) | 1.7 | `Window.{FreezePanes,SplitRow,SplitColumn}` | `No` | `WorksheetFreezePanes` (5 domain methods) |
| `horizontalPageBreaks` / `verticalPageBreaks` | prop(ro) | 1.9 | `_Worksheet.HPageBreaks` / `VPageBreaks` | `No` | 2 members |
| `namedSheetViews` | prop(ro) | Online 1.1 | `_Worksheet.NamedSheetViews (Get)` | `No` | — |
| `names` | prop(ro) | 1.4 | `_Worksheet.Names (Get)` | `No` | Workbook `Names` only today (`dc-hsa5.6`) |
| `notes` | prop(ro) | 1.18 | `_Worksheet.Comments (Get)` (legacy notes) | `No` | — |
| `pageLayout` | prop(ro) | 1.9 | `_Worksheet.PageSetup (Get)` | `No` | `PageSetup` not in descriptor |
| `pivotTables` | prop(ro) | 1.3 | `_Worksheet.PivotTables (Method)` | `No` | — |
| `protection` | prop(ro) | 1.2 | `_Worksheet.Protection` / `Protect` / `Unprotect` | `No` | `WorksheetProtection` (14 domain members) |
| `shapes` | prop(ro) | 1.9 | `_Worksheet.Shapes (Get)` | `Impl` | `automation/report.rs` (`Self::sheet_shapes`) |
| `slicers` | prop(ro) | 1.10 | `None` (COM uses `SlicerCaches`) | `No` | — |
| `tables` | prop(ro) | 1.1 | `_Worksheet.ListObjects (Get)` | `Impl` | `automation/report.rs` (`Self::sheet_list_objects`) |
| `enableCalculation` | prop(rw) | 1.9 | `_Worksheet.EnableCalculation (Get/Put)` | `No` | — |
| `id` | prop(ro) | 1.1 | `None (Office.js-only)` (GUID) | `No` | `SheetId` exists internally |
| `name` | prop(rw) | 1.1 | `_Worksheet.Name (Get/Put)` | `Partial` | `Get` implemented (`automation.rs`); `Put` not registered |
| `position` | prop(rw) | 1.1 | `_Worksheet.Index (Get)` (1-based vs 0-based) + `Move` | `Partial` | `Index (Get)` implemented (`navigation.rs`); `Put` not registered |
| `showDataTypeIcons` | prop(rw) | 1.19 | `Window.DisplayDataTypeIcons (Get/Put)` | `No` | — |
| `showGridlines` | prop(rw) | 1.8 | `Window.DisplayGridlines (Get/Put)` | `No` | — |
| `showHeadings` | prop(rw) | 1.8 | `Window.DisplayHeadings (Get/Put)` | `No` | — |
| `standardHeight` | prop(ro) | 1.7 | `_Worksheet.StandardHeight (Get)` | `No` | — |
| `standardWidth` | prop(rw) | 1.7 | `_Worksheet.StandardWidth (Get/Put)` | `No` | — |
| `tabColor` | prop(rw) | 1.7 | `COM-absent (Tab.Color)` | `No` | `_Worksheet.Tab` in descriptor; `Tab` interface absent |
| `tabId` | prop(ro) | 1.14 | `None (Office.js-only)` | `No` | XML `sheetId` |
| `visibility` | prop(rw) | 1.1 | `_Worksheet.Visible (Get/Put)` | `No` | — |
| `activate` | method | 1.1 | `_Worksheet.Activate (Method)` | `No` | — |
| `calculate` | method | 1.6 | `_Worksheet.Calculate (Method)` | `Impl` | `automation/mutation.rs` (`Self::calculate_sheet`) |
| `checkSpelling` | method | Desktop 1.1 | `_Worksheet.CheckSpelling (Method)` | `No` | — |
| `clearArrows` | method | Desktop 1.1 | `_Worksheet.ClearArrows (Method)` | `No` | — |
| `copy` | method | 1.7 | `_Worksheet.Copy (Method)` | `No` | — |
| `delete` | method | 1.1 | `_Worksheet.Delete (Method)` | `No` | — |
| `evaluate` | method | Desktop 1.1 | `_Worksheet.Evaluate (Method)` | `No` | — |
| `findAll` / `findAllOrNullObject` | method | 1.9 | `Range.Find` (cells) | `No` | 2 members |
| `getCell` | method | 1.1 | `_Worksheet.Cells (Get)` + `Range.Item` | `Impl` (via `Cells`) | OJS 0-based `(row, col)` vs COM 1-based |
| `getNext` / `getNextOrNullObject` / `getPrevious` / `getPreviousOrNullObject` | method | 1.5 | `_Worksheet.Next (Get)` / `Previous (Get)` | `No` | 4 members (`*OrNullObject` is OJS-only) |
| `getRange` | method | 1.1 | `_Worksheet.Range (Get)` | `Impl` | `automation/navigation.rs` (`Self::worksheet_range`) |
| `getRangeByIndexes` | method | 1.7 | `_Worksheet.Cells` + `Range.Resize` | `Impl` (composed) | 0-based `(startRow, startCol, rowCount, colCount)` |
| `getRanges` | method | 1.9 | `_Worksheet.Range (Get)` (multi-area) | `Impl` (via `Range`) | Returns `RangeAreas` |
| `getUsedRange` / `getUsedRangeOrNullObject` | method | 1.1 / 1.4 | `_Worksheet.UsedRange (Get)` | `No` | 2 members (`*OrNullObject` is OJS-only) |
| `replaceAll` | method | 1.9 | `Range.Replace` | `No` | — |
| `showOutlineLevels` | method | 1.10 | `COM-absent (Outline.ShowLevels)` | `No` | `_Worksheet.Outline` in descriptor; `Outline` interface absent |
| `onActivated`, `onCalculated`, `onChanged`, `onColumnSorted`, `onDeactivated`, `onFormatChanged`, `onFormulaChanged`, `onNameChanged`, `onProtectionChanged`, `onRowHiddenChanged`, `onRowSorted`, `onSelectionChanged`, `onSingleClicked`, `onVisibilityChanged` | event (×14) | 1.7–1.17 | `DocEvents.*` (5 match: `Activate`, `Calculate`, `Change`, `Deactivate`, `SelectionChange`) | `Event-src` (2: `WorksheetChange`, `WorksheetCalculate`) | See §12 |

---

## 7. `Range`, `RangeAreas`, `RangeView`, and `RangeFormat`

### 7.1 `Range` (111 domain members + 6 boilerplate = 117)

| Office.js Member | Kind | Req Set | COM Descriptor Member | `calc-host` Today | Notes |
|---|---|---|---|---|---|
| `conditionalFormats` | prop(ro) | 1.6 | `Range.FormatConditions (Get)` | `No` | 18 `ConditionalFormat*` classes |
| `dataValidation` | prop(ro) | 1.8 | `Range.Validation (Get)` | `No` | `Validation` interface not in descriptor |
| `format` | prop(ro) | 1.1 | `Range.{Font,Interior,Borders,HorizontalAlignment,...}` | `No` | Separate `RangeFormat` sub-object in OJS |
| `sort` | prop(ro) | 1.2 | `Range.Sort (Method)` | `No` | `RangeSort` sub-object in OJS |
| `worksheet` | prop(ro) | 1.1 | `Range.Worksheet (Get)` | `Impl` | `automation.rs` (`Self::range_sheet`) |
| `address` | prop(ro) | 1.1 | `Range.Address (Get)` | `Impl` | `automation.rs` (`Self::range_address`); OJS includes `Sheet!` prefix |
| `addressLocal` | prop(ro) | 1.1 | `Range.AddressLocal (Get)` | `No` | — |
| `cellCount` | prop(ro) | 1.1 | `Range.CountLarge (Get)` / `Count (Get)` | `Impl` | `automation.rs` (`Self::range_count`) |
| `columnCount` | prop(ro) | 1.1 | `Range.Columns.Count` | `Impl` | `Range.Columns (Get)` + `Range.Count (Get)` |
| `columnHidden` | prop(rw) | 1.2 | `Range.EntireColumn.Hidden (Get/Put)` | `No` | — |
| `columnIndex` | prop(ro) | 1.1 | `Range.Column (Get)` | `Impl` | 0-based in OJS vs 1-based in COM (`Self::range_column`) |
| `control` | prop(rw) | 1.18 | `Range.CellControl (Get)` | `No` | `CheckboxCellControl` |
| `formulaArray` | prop(rw) | Desktop 1.1 | `Range.FormulaArray (Get/Put)` | `No` | — |
| `formulas` | prop(rw) | 1.1 | `Range.Formula (Get/Put)` | `Partial` | `automation/formula.rs` (`Range.Formula` Get/Put; OJS always returns 2-D array) |
| `formulasLocal` | prop(rw) | 1.1 | `Range.FormulaLocal (Get/Put)` | `No` | — |
| `formulasR1C1` | prop(rw) | 1.2 | `Range.FormulaR1C1 (Get/Put)` | `No` | — |
| `hasSpill` | prop(ro) | 1.12 | `Range.HasSpill (Get)` | `No` | — |
| `height` | prop(ro) | 1.10 | `Range.Height (Get)` | `No` | — |
| `hidden` | prop(ro) | 1.2 | `Range.Hidden (Get)` | `No` | — |
| `hyperlink` | prop(rw) | 1.7 | `Range.Hyperlinks (Get)` | `No` | — |
| `isEntireColumn` | prop(ro) | 1.7 | `None (Office.js-only)` | `No` | — |
| `isEntireRow` | prop(ro) | 1.7 | `None (Office.js-only)` | `No` | — |
| `left` | prop(ro) | 1.10 | `Range.Left (Get)` | `No` | — |
| `linkedDataTypeState` | prop(ro) | 1.9 | `Range.LinkedDataTypeState (Get)` | `No` | — |
| `numberFormat` | prop(rw) | 1.1 | `Range.NumberFormat (Get/Put)` | `No` | — |
| `numberFormatCategories` | prop(ro) | 1.12 | `None (Office.js-only)` | `No` | — |
| `numberFormatLocal` | prop(rw) | 1.7 | `Range.NumberFormatLocal (Get/Put)` | `No` | — |
| `rowCount` | prop(ro) | 1.1 | `Range.Rows.Count` | `Impl` | `Range.Rows (Get)` + `Range.Count (Get)` |
| `rowHidden` | prop(rw) | 1.2 | `Range.EntireRow.Hidden (Get/Put)` | `No` | — |
| `rowIndex` | prop(ro) | 1.1 | `Range.Row (Get)` | `Impl` | 0-based in OJS vs 1-based in COM (`Self::range_row`) |
| `savedAsArray` | prop(ro) | 1.12 | `Range.SavedAsArray (Get)` | `No` | — |
| `style` | prop(rw) | 1.7 | `Range.Style (Get/Put)` | `No` | — |
| `text` | prop(ro) | 1.1 | `Range.Text (Get)` | `Partial` | `automation/formula.rs` (`Self::cells_text`; COM returns `Null` if cells differ, OJS returns 2-D `string[][]`) |
| `top` | prop(ro) | 1.10 | `Range.Top (Get)` | `No` | — |
| `valueTypes` | prop(ro) | 1.1 | `None (Office.js-only)` | `No` | Cell type classification array |
| `values` | prop(rw) | 1.1 | `Range.Value2 (Get/Put)` | `Partial` | `automation.rs` (`Self::range_value2`) & `mutation.rs` (`put_value2`); JS primitives match `Value2` (no `VT_DATE`/`VT_CY`); text-entry/shape limits (`dc-1nbp.6`) |
| `valuesAsJson` / `valuesAsJsonLocal` | prop(rw) | 1.16 | `None (Office.js-only)` | `No` | 2 members; rich `CellValue` JSON |
| `width` | prop(ro) | 1.10 | `Range.Width (Get)` | `No` | — |
| `autoFill` | method | 1.9 | `Range.AutoFill (Method)` | `No` | — |
| `calculate` | method | 1.6 | `Range.Calculate (Method)` | `Impl` | `automation/mutation.rs` (`Self::calculate_range`) |
| `checkSpelling` | method | Desktop 1.1 | `Range.CheckSpelling (Method)` | `No` | — |
| `clear` | method | 1.1 | `Range.Clear` / `ClearContents` / `ClearFormats` / `ClearHyperlinks` | `Partial` | Only `Range.ClearContents` implemented (`mutation.rs`); `Clear`/`ClearFormats` unmodelled |
| `clearOrResetContents` | method | 1.18 | `Range.ResetContents (Method)` | `No` | — |
| `convertDataTypeToText` / `convertToLinkedDataType` | method | 1.9 | `Range.DataTypeToText` / `ConvertToLinkedDataType` | `No` | 2 members |
| `copyFrom` | method | 1.9 | `Range.Copy` + `PasteSpecial` | `No` | — |
| `delete` | method | 1.1 | `Range.Delete (Method)` | `No` | — |
| `find` / `findOrNullObject` | method | 1.9 | `Range.Find (Method)` | `No` | 2 members |
| `flashFill` | method | 1.9 | `Range.FlashFill (Method)` | `No` | — |
| `getAbsoluteResizedRange` | method | 1.7 | `Range.Resize (Get)` (on top-left cell) | `Impl` (composed) | — |
| `getBoundingRect` | method | 1.1 | `None (Office.js-only)` | `No` | — |
| `getCell` | method | 1.1 | `Range.Item (Get)` | `Impl` | 0-based in OJS vs 1-based in COM (`navigation.rs`) |
| `getCellProperties` / `setCellProperties` / `getDisplayedCellProperties` | method | 1.9 / 1.19 | `None (Office.js-only)` | `No` | 3 members (batched property bag) |
| `getColumn` / `getRow` | method | 1.1 | `Range.Columns.Item` / `Range.Rows.Item` | `Impl` (via `Rows`/`Columns`+`Item`) | 2 members (0-based in OJS) |
| `getColumnProperties` / `setColumnProperties` / `getRowProperties` / `setRowProperties` | method | 1.9 | `None (Office.js-only)` | `No` | 4 members |
| `getColumnsAfter` / `getColumnsBefore` / `getRowsAbove` / `getRowsBelow` | method | 1.2 | `None (Office.js-only)` | `No` | 4 members (composable from `Offset`/`Resize`) |
| `getDependents` / `getDirectDependents` / `getDirectPrecedents` / `getPrecedents` | method | 1.12–1.15 | `Range.Dependents` / `DirectDependents` / `DirectPrecedents` / `Precedents` | `No` | 4 members |
| `getEntireColumn` / `getEntireRow` | method | 1.1 | `Range.EntireColumn (Get)` / `EntireRow (Get)` | `Impl` | 2 members (`navigation.rs`) |
| `getExtendedRange` / `getRangeEdge` | method | 1.13 | `Range.End (Get)` | `No` | 2 members |
| `getImage` | method | 1.7 | `None (Office.js-only)` | `No` | — |
| `getIntersection` / `getIntersectionOrNullObject` | method | 1.1 / 1.4 | `_Application.Intersect (Method)` | `No` | 2 members (`Intersect` not implemented in `calc-host`) |
| `getLastCell` / `getLastColumn` / `getLastRow` | method | 1.1 | `None (Office.js-only)` | `No` | 3 members |
| `getMergedAreasOrNullObject` | method | 1.13 | `Range.MergeArea (Get)` | `No` | — |
| `getOffsetRange` | method | 1.1 | `Range.Offset (Get)` | `Impl` | `navigation.rs` (`Self::range_offset`) |
| `getPivotTables` | method | 1.12 | `Range.PivotTable (Get)` | `No` | — |
| `getResizedRange` | method | 1.2 | `Range.Resize (Get)` (delta vs absolute) | `Impl` (delta-adjusted) | `navigation.rs` (`Self::range_resize`) |
| `getSpecialCells` / `getSpecialCellsOrNullObject` | method | 1.9 | `Range.SpecialCells (Method)` | `No` | 2 members |
| `getSpillParent` / `*OrNullObject` / `getSpillingToRange` / `*OrNullObject` | method | 1.12 | `Range.SpillParent` / `SpillingToRange` | `No` | 4 members |
| `getSurroundingRegion` | method | 1.7 | `Range.CurrentRegion (Get)` | `No` | — |
| `getTables` | method | 1.9 | `Range.ListObject (Get)` | `No` | — |
| `getUsedRange` / `getUsedRangeOrNullObject` | method | 1.1 / 1.4 | `None` (COM `UsedRange` is on `_Worksheet`) | `No` | 2 members |
| `getVisibleView` | method | 1.3 | `None (Office.js-only)` | `No` | Returns `RangeView` |
| `group` / `ungroup` / `hideGroupDetails` / `showGroupDetails` | method | 1.10 | `Range.Group` / `Ungroup` / `ShowDetail` | `No` | 4 members |
| `insert` | method | 1.1 | `Range.Insert (Method)` | `No` | — |
| `merge` / `unmerge` | method | 1.2 | `Range.Merge` / `UnMerge` | `No` | 2 members |
| `moveTo` | method | 1.11 | `Range.Cut(Destination)` | `No` | — |
| `removeDuplicates` | method | 1.9 | `Range.RemoveDuplicates (Method)` | `No` | — |
| `replaceAll` | method | 1.9 | `Range.Replace (Method)` | `No` | — |
| `select` | method | 1.1 | `Range.Select (Method)` | `No` | — |
| `setDirty` | method | 1.9 | `Range.Dirty (Method)` | `No` | — |
| `showCard` | method | 1.7 | `Range.ShowCard (Method)` | `No` | — |
| `showDependents` / `showPrecedents` | method | Desktop 1.1 | `Range.ShowDependents` / `ShowPrecedents` | `No` | 2 members |

### 7.2 `RangeAreas`, `RangeView`, and Formatting Sub-Objects

- **`RangeAreas`** (`ExcelApi 1.9`, 30 domain + 6 boilerplate = 36): multi-area range wrapper (returned by `Application.union`, `Worksheet.getRanges`, `Range.getSpecialCells`). `areas` maps to `Range.Areas (Get)` (**Impl** in `navigation.rs`), `areaCount` to `Areas.Count (Get)` (**Impl**), `cellCount` to `Range.Count (Get)` (**Impl**), `address`/`calculate`/`clear`/`getEntireColumn`/`getEntireRow`/`getOffsetRangeAreas` mirror `Range` across areas; remaining 20+ members unmodelled. `WorkbookRangeAreas` (5 domain, `1.12`) is Office.js-only.
- **`RangeView`** (`ExcelApi 1.3`, 15 domain + 4 boilerplate = 19) & `RangeViewCollection` (2 domain): visible-cell projection; no COM counterpart (`None`).
- **`RangeFormat`** (`ExcelApi 1.1`, 19 domain + 4 boilerplate = 23) + `RangeFill` (6), `RangeBorder` (5), `RangeBorderCollection` (4), `RangeFont` (10), `FormatProtection` (2): map to COM `Range.{Interior, Borders, Font, HorizontalAlignment, VerticalAlignment, WrapText, ColumnWidth, RowHeight, ShrinkToFit, ReadingOrder, Orientation, IndentLevel, AddIndent, UseStandardHeight, UseStandardWidth, Locked, FormulaHidden, AutoFit}` (with `Font`/`Interior`/`Borders` interfaces not yet in `descriptor/excel.interfaces.txt`). `calc-host` implements **0** formatting members.
- **`RangeSort`** (1), `DataValidation` (9), `ConditionalFormat*` (18 classes, 130+ domain members): map to COM `Range.Sort`, `Range.Validation`, `Range.FormatConditions` (interfaces absent from descriptor; **0** implemented in `calc-host`).

---

## 8. `Table` (`ListObject`)

### 8.1 `TableCollection` (9 domain + 4) & `TableScopedCollection` (4 domain + 4)

| Office.js Member | Kind | Req Set | COM Descriptor Member | `calc-host` Today | Notes |
|---|---|---|---|---|---|
| `TableCollection.count` / `getCount` | prop / meth | 1.1 / 1.4 | `ListObjects.Count (Get)` | `Impl` | `automation/report.rs` (`Self::tables_count`) |
| `TableCollection.add` | method | 1.1 | `ListObjects.Add (Method)` | `Partial` | `Self::tables_add`; refuses `xlNo` / non-text headers (`OBJECT_MODEL.md` §10a) |
| `TableCollection.getItem` / `getItemAt` | method | 1.1 | `ListObjects.Item (Get)` | `Impl` | `Self::tables_item` (by name or 1-based index; OJS `getItemAt` is 0-based) |
| `TableCollection.getItemOrNullObject` | method | 1.4 | `None (Office.js-only)` | `No` | — |
| `TableCollection.onAdded` / `onChanged` / `onDeleted` | event (×3) | 1.7 / 1.9 | `COM-absent` (`AppEvents.SheetTableUpdate` is closest) | `No` | See §12 |
| `TableScopedCollection` (`getCount`, `getFirst`, `getItem`, `getItemOrNullObject`) | method (×4) | 1.9 / 1.14 | `None (Office.js-only)` | `No` | Workbook-level table collection |

### 8.2 `Table` (27 domain members + 4 boilerplate = 31)

| Office.js Member | Kind | Req Set | COM Descriptor Member | `calc-host` Today | Notes |
|---|---|---|---|---|---|
| `autoFilter` | prop(ro) | 1.9 | `ListObject.AutoFilter (Get)` | `No` | — |
| `columns` | prop(ro) | 1.1 | `ListObject.ListColumns (Get)` | `Impl` | `automation/report.rs` (`Self::table_columns`) |
| `rows` | prop(ro) | 1.1 | `ListObject.ListRows (Get)` | `Impl` | `automation/report.rs` (`Self::table_rows`) |
| `sort` | prop(ro) | 1.2 | `ListObject.Sort (Get)` | `No` | — |
| `worksheet` | prop(ro) | 1.2 | `ListObject.Parent (Get)` | `No` | `ListObject.Parent` not registered in `report.rs` |
| `highlightFirstColumn` / `highlightLastColumn` | prop(rw) | 1.3 | `ListObject.ShowTableStyleFirstColumn` / `LastColumn` | `No` | 2 members |
| `id` / `legacyId` | prop(ro) | 1.1 / 1.8 | `None (Office.js-only)` | `No` | 2 members |
| `name` | prop(rw) | 1.1 | `ListObject.Name (Get/Put)` / `DisplayName` | `Partial` | `Name` & `DisplayName` Get/Put in `report.rs`; non-defined-name shapes refused |
| `showBandedColumns` / `showBandedRows` | prop(rw) | 1.3 | `ListObject.ShowTableStyleColumnStripes` / `RowStripes` | `No` | 2 members |
| `showFilterButton` | prop(rw) | 1.3 | `ListObject.ShowAutoFilterDropDown (Get/Put)` | `No` | — |
| `showHeaders` | prop(rw) | 1.1 | `ListObject.ShowHeaders (Get/Put)` | `No` | — |
| `showTotals` | prop(rw) | 1.1 | `ListObject.ShowTotals (Get/Put)` | `Impl` | `report.rs` (`Self::table_show_totals` / `put_show_totals`) |
| `style` | prop(rw) | 1.1 | `ListObject.TableStyle (Get/Put)` | `Impl` | `report.rs` (`Self::table_style` / `put_style`) |
| `clearFilters` / `reapplyFilters` | method | 1.2 | `COM-absent (AutoFilter.ShowAllData / ApplyFilter)` | `No` | 2 members |
| `convertToRange` | method | 1.2 | `ListObject.Unlist (Method)` | `Impl` | `report.rs` (`Self::table_unlist`) |
| `delete` | method | 1.1 | `ListObject.Delete (Method)` | `Impl` | `report.rs` (`Self::table_delete`) |
| `getDataBodyRange` | method | 1.1 | `ListObject.DataBodyRange (Get)` | `Impl` | `report.rs` (`Self::table_body`) |
| `getHeaderRowRange` | method | 1.1 | `ListObject.HeaderRowRange (Get)` | `Impl` | `report.rs` (`Self::table_header`) |
| `getRange` | method | 1.1 | `ListObject.Range (Get)` | `Impl` | `report.rs` (`Self::table_range`) |
| `getTotalRowRange` | method | 1.1 | `ListObject.TotalsRowRange (Get)` | `No` | — |
| `resize` | method | 1.13 | `ListObject.Resize (Method)` | `No` | — |
| `onChanged` / `onSelectionChanged` | event (×2) | 1.7 | `COM-absent` | `No` | See §12 |

### 8.3 `TableColumn*`, `TableRow*`, `TableSort`, `Filter`, `AutoFilter`

- **`TableColumnCollection`** (7 domain: `count`, `getCount`, `getItem`, `getItemAt`, `getItemOrNullObject`, `add`, `addAsJson`): `count`/`getCount` (`ListColumns.Count`) and `getItem`/`getItemAt` (`ListColumns.Item`) **Impl** in `report.rs`.
- **`TableColumn`** (12 domain): `name` (`ListColumn.Name` Get/Put **Impl**), `index` (`ListColumn.Index`), `delete` (`ListColumn.Delete`), `getDataBodyRange`/`getRange` (`ListColumn.DataBodyRange`/`Range`), `getHeaderRowRange`/`getTotalRowRange` (composed), `values`/`valuesAsJson*`, `filter` (`Filter`), `id` (OJS-only).
- **`TableRowCollection`** (7 domain): `count`/`getCount` (`ListRows.Count` **Impl**), `add` (`ListRows.Add` **Impl**), `getItemAt` (`ListRows.Item`), `addAsJson`, `deleteRows`/`deleteRowsAt` (`ExcelApiOnline 1.1`).
- **`TableRow`** (6 domain): `index` (`ListRow.Index`), `delete` (`ListRow.Delete`), `getRange` (`ListRow.Range`), `values`/`valuesAsJson*`. **`ListRow` has 0 handlers in `calc-host` today**.
- **`TableSort`** (6 domain), **`Filter`** (13 domain), **`AutoFilter`** (10 domain): map to COM `Sort`/`AutoFilter` (interfaces absent from descriptor; **0** implemented in `calc-host`).

---

## 9. `Chart` and Chart Sub-Objects

COM splits `ChartObjects` (the worksheet's container of `ChartObject` frames) from `_Chart` (the chart itself), whereas Office.js merges frame geometry (`left`, `top`, `width`, `height`, `name`, `delete`) directly onto `Excel.Chart`.

### 9.1 `ChartCollection` (10 domain + 4)

| Office.js Member | Kind | Req Set | COM Descriptor Member | `calc-host` Today | Notes |
|---|---|---|---|---|---|
| `count` / `getCount` | prop / meth | 1.1 / 1.4 | `ChartObjects.Count (Get)` | `Impl` | `report.rs` (`Self::chart_objects_count`) |
| `add` | method | 1.1 | `ChartObjects.Add (Method)` + `_Chart.ChartType` + `_Chart.SetSourceData` | `Impl` | `report.rs` (`chart_objects_add` + `chart_put_type` + `chart_set_source`) |
| `getItem` / `getItemAt` | method | 1.1 | `ChartObjects.Item (Method)` | `Impl` | `report.rs` (`Self::chart_objects_item`) |
| `getItemOrNullObject` | method | 1.4 | `None (Office.js-only)` | `No` | — |
| `onActivated` / `onAdded` / `onDeactivated` / `onDeleted` | event (×4) | 1.8 | `COM-absent` (`WorkbookEvents.NewChart` for add only) | `No` | See §12 |

### 9.2 `Chart` (33 domain members + 4 boilerplate = 37)

| Office.js Member | Kind | Req Set | COM Descriptor Member | `calc-host` Today | Notes |
|---|---|---|---|---|---|
| `axes` / `dataLabels` / `format` / `pivotOptions` / `plotArea` | prop(ro) | 1.1–1.9 | `_Chart.{Axes, ChartArea, PivotLayout, PlotArea}` | `No` | 5 sub-objects (`Axes` in descriptor with 0 handlers) |
| `legend` | prop(ro) | 1.1 | `_Chart.Legend (Get)` + `HasLegend (Get/Put)` | `Partial` | `HasLegend` Get/Put **Impl** (`report.rs`); `Legend` object (0 handlers) |
| `series` | prop(ro) | 1.1 | `_Chart.SeriesCollection (Method)` | `Impl` | `report.rs` (`Self::chart_series_collection`) |
| `title` | prop(ro) | 1.1 | `_Chart.ChartTitle (Get)` + `HasTitle (Get/Put)` | `Impl` | `report.rs` (`Self::chart_title`, `chart_has_title`) |
| `worksheet` | prop(ro) | 1.2 | `_Chart.Parent (Get)` / `ChartObject.Parent` | `No` | — |
| `chartType` | prop(rw) | 1.7 | `_Chart.ChartType (Get/Put)` | `Impl` | `report.rs` (`Self::chart_type` / `chart_put_type`) |
| `plotBy` | prop(rw) | 1.8 | `_Chart.PlotBy (Get/Put)` | `Impl` | `report.rs` (`Self::chart_plot_by` / `chart_put_plot_by`) |
| `left` / `top` / `width` / `height` / `name` | prop(rw) | 1.1 | `ChartObject.{Left,Top,Width,Height,Name} (Get/Put)` | `Impl` | 5 members **Impl** on `ChartObject` in `report.rs` |
| `setPosition` | method | 1.1 | `ChartObject.{Left,Top,Width,Height}` + `BottomRightCell`/`TopLeftCell` | `Impl` (via geometry puts) | — |
| `setData` | method | 1.1 | `_Chart.SetSourceData (Method)` | `Impl` | `report.rs` (`Self::chart_set_source`) |
| `activate` / `delete` / `getImage` / `getDataTable*` | method | 1.1–1.14 | `_Chart.{Activate, Delete}` / `ChartObject.Delete` / `ChartDataTable` | `No` | 5 members |
| `categoryLabelLevel` / `displayBlanksAs` / `id` / `plotVisibleOnly` / `seriesNameLevel` / `showAllFieldButtons` / `showDataLabelsOverMaximum` / `style` | prop | 1.7–1.8 | `_Chart.*` (all except `id`) | `No` | 8 properties |
| `onActivated` / `onDeactivated` | event (×2) | 1.8 | `COM-absent` | `No` | See §12 |

### 9.3 `ChartTitle`, `ChartSeries*`, `ChartAxes`, `ChartAxis`, `ChartLegend` & Remaining 40 Chart Classes

- **`ChartTitle`** (15 domain): `text` (`ChartTitle.Text` Get/Put **Impl** in `report.rs`), `visible` (`_Chart.HasTitle` Get/Put **Impl** in `report.rs`); remaining 13 (`format`, `height`, `width`, `left`, `top`, `overlay`, `position`, `horizontalAlignment`, `verticalAlignment`, `showShadow`, `textOrientation`, `getSubstring`, `setFormula`) map to COM `ChartTitle` (0 handlers).
- **`ChartSeriesCollection`** (4 domain): `count`/`getCount` (`SeriesCollection.Count` **Impl**), `getItemAt` (`SeriesCollection.Item` **Impl**), `add` (`SeriesCollection.Add` / `NewSeries`).
- **`ChartSeries`** (53 domain): `name` (`Series.Name` Get **Impl**, Put unmodelled), `plotOrder` (`Series.PlotOrder` Get **Impl**), `setValues` (`Series.Values` Put **Impl**), `setXAxisValues` (`Series.XValues` Put **Impl**); remaining 49 (`chartType`, `axisGroup`, `dataLabels`, `points`, `trendlines`, `xErrorBars`/`yErrorBars`, `marker*`, `gradient*`, `delete`, etc.) map mostly to COM `Series` (0 handlers) and `ExcelApi 1.9`–`1.15` sub-objects.
- **`ChartAxes`** (4 domain), **`ChartAxis`** (42 domain), **`ChartLegend`** (10 domain): map to COM `Axes`, `Axis`, `Legend` (all three in `excel.json` with **0** handlers in `calc-host`).
- **Remaining 40 Chart sub-classes** (388 domain members: `ChartAxisTitle`, `ChartDataLabels`, `ChartDataLabel`, `ChartGridlines`, `ChartPointsCollection`, `ChartPoint`, `ChartPlotArea`, `ChartTrendline*`, `ChartErrorBars*`, `ChartDataTable*`, `Chart*Format`, `ChartFill`, `ChartBorder`, `ChartFont`, `ChartLeaderLines*`, etc.): mostly map to COM-absent chart formatting interfaces; **0** implemented in `calc-host`.

---

## 10. `NamedItem` (`Names` / `Name`)

| Office.js Member | Kind | Req Set | COM Descriptor Member | `calc-host` Today | Notes |
|---|---|---|---|---|---|
| `NamedItemCollection.add` / `addFormulaLocal` | method | 1.4 | `Names.Add (Method)` | `No` | — |
| `NamedItemCollection.getCount` | method | 1.4 | `Names.Count (Get)` | `Partial` | `navigation.rs` (`Self::names_count`); refuses if sheet-local names exist (`dc-hsa5.6`) |
| `NamedItemCollection.getItem` | method | 1.1 | `Names.Item (Method)` | `Partial` | `navigation.rs` (`Self::names_item`); global names only (`dc-hsa5.6`) |
| `NamedItemCollection.getItemOrNullObject` | method | 1.4 | `None (Office.js-only)` | `No` | — |
| `NamedItem.name` | prop(ro) | 1.1 | `Name.Name (Get)` | `Partial` | `navigation.rs` (`Self::name_text`); global only |
| `NamedItem.formula` | prop(rw) | 1.7 | `Name.RefersTo (Get/Put)` | `Partial` | `Get` (`Self::name_formula`) **Impl**; `Put` not registered |
| `NamedItem.getRange` | method | 1.1 | `Name.RefersToRange (Get)` | `Partial` | `navigation.rs` (`Self::name_range`); refuses if shadowed by sheet-local name |
| `NamedItem.getRangeOrNullObject` | method | 1.4 | `None (Office.js-only)` | `No` | — |
| `NamedItem.comment` / `visible` / `delete` | prop/meth | 1.1–1.4 | `Name.{Comment, Visible, Delete}` | `No` | 3 members |
| `NamedItem.scope` / `type` / `value` / `valueAsJson*` / `arrayValues` / `worksheet*` | prop | 1.1–1.16 | `None` (`worksheet` via `Name.Parent` when sheet-scoped) | `No` | 8 members (`NamedItemArrayValues` has 3 domain members) |

---

## 10a. `Shape` and Drawing (`calc-host` `report_draw.rs` Alignment)

`calc-host/src/automation/report_draw.rs` implements 46+ COM members on `Shapes`, `Shape`, `TextFrame2`, `TextRange2`, `FillFormat`, `LineFormat`, `ColorFormat`, and `ConnectorFormat`. Mapping Office.js `Shape*` (`ExcelApi 1.9`) to those COM owners:

- **`ShapeCollection`** (9 domain):
  - **Impl in COM**: `getCount` (`Shapes.Count`), `getItem` / `getItemAt` (`Shapes.Item`), `addGeometricShape` (`Shapes.AddShape`), `addLine` (`Shapes.AddLine` + `Shapes.AddConnector`), `addTextBox` (`Shapes.AddTextbox`), plus `addChart2` lives on `Shapes.AddChart2` in COM (`ChartCollection.add` in Office.js).
  - **Not in `calc-host`**: `addGroup`, `addImage`, `getItemOrNullObject`.
- **`Shape`** (37 domain):
  - **Impl in COM**: `name` (`Shape.Name` Get/Put), `id` (`Shape.ID`), `left` / `top` / `width` / `height` (`Shape.{Left,Top,Width,Height}` Get/Put), `delete` (`Shape.Delete`), `type` (`Shape.Type`), `geometricShapeType` (`Shape.AutoShapeType` Get), `connectionSiteCount` (`Shape.ConnectionSiteCount`), `fill` (`Shape.Fill`), `lineFormat` (`Shape.Line`), `line` (`Shape.Connector` / `Shape.ConnectorFormat`), `textFrame` (`Shape.TextFrame2`).
  - **Not in `calc-host`**: `altText*`, `displayName`, `group`/`parentGroup`/`geometricShape`/`image`, `level`, `lockAspectRatio`, `placement`, `rotation`, `visible`, `zOrderPosition`, `copyTo`, `getAsImage`, `increment*`, `scale*`, `setZOrder`, `onActivated`, `onDeactivated`.
- **`Line`** (19 domain): maps to COM `ConnectorFormat` (**Impl** in `report_draw.rs`: `beginConnectedShape`, `endConnectedShape`, `beginConnectedSite`, `endConnectedSite`, `isBeginConnected`, `isEndConnected`, `connectBeginShape`, `connectEndShape`; unmodelled: `disconnectBeginShape`, `disconnectEndShape`, `connectorType`, arrowhead properties).
- **`ShapeFill`** (5 domain): `foregroundColor` / `setSolidColor` map to COM `FillFormat.ForeColor` (`ColorFormat.RGB` **Impl**); `transparency`, `type`, `clear` unmodelled.
- **`ShapeLineFormat`** (6 domain): `color` (`LineFormat.ForeColor` -> `ColorFormat.RGB` **Impl**), `weight` (`LineFormat.Weight` **Impl**); `dashStyle`, `style`, `transparency`, `visible` unmodelled.
- **`TextFrame`** (14 domain) & **`TextRange`** (3 domain): `TextFrame.textRange` (`TextFrame2.TextRange` **Impl**) and `TextRange.text` (`TextRange2.Text` Get/Put **Impl**); margins/alignment/font/overflow unmodelled.

---

## 11. `Functions` (`workbook.functions`) and `CustomFunctions`

- **`Excel.Functions`** (`ExcelApi 1.2`, **368** unique members = `context`, `toJSON`, plus **366 worksheet function methods**):
  - Maps to COM `WorksheetFunction` (420 members in `excel.json`: 417 methods + `Application`, `Parent`, `Creator`).
  - **Exact normalized overlap**: **302** methods appear on both `Excel.Functions` and COM `WorksheetFunction`.
  - **In `Excel.Functions` only (64 methods)**: `abs`, `areas`, `atan`, `averageA`, `char`, `code`, `columns`, `concatenate`, `cos`, `date`, `datevalue`, `day`, `ecma_Ceiling`, `error_Type`, `exact`, `exp`, `false`, `hour`, `hyperlink`, `if`, `int`, `isref`, `left`, `leftb`, `len`, `lenb`, `lower`, `maxA`, `mid`, `midb`, `minA`, `minute`, `mod`, `month`, `n`, `na`, `not`, `now`, `rand`, `right`, `rightb`, `rows`, `second`, `sheet`, `sheets`, `sign`, `sin`, `sqrt`, `stDevA`, `stDevPA`, `t`, `tan`, `time`, `timevalue`, `today`, `true`, `trunc`, `type`, `upper`, `value`, `varA`, `varPA`, `year`, `yield` (omitted from COM `WorksheetFunction` because VBA provides language built-ins, but present in `calc-func`'s worksheet catalogue!).
  - **In COM `WorksheetFunction` only (93+ methods)**: legacy compatibility functions (`ChiDist`, `FTest`, `LogInv`, etc.) and newer functions (`Aggregate`, `ArrayToText`, `Concat`, `Filter`, `Forecast_ETS*`, `IfError`, `IfNa`, `Index`, `MaxIfs`, etc.).
  - **`calc-host` / `calc-catalogue` today**: `crates/calc-catalogue` contains string definitions for all **366/366** `Excel.Functions` names (and `OBJECT_MODEL.md` §10 records 402/417 COM `WorksheetFunction` methods resolving in `calc-catalogue`). In `WorkbookAutomation::worksheet_functions`, calls go through `UserFunctionContext::worksheet_function` inside a formula invocation or `calc_func::invoke` against `SessionCells` in command/event contexts.
- **`FunctionResult<T>`** (`ExcelApi 1.2`, 2 domain members: `error`, `value`): `None (Office.js-only)` proxy result wrapper.
- **`CustomFunctions`** (`@types/custom-functions-runtime`, separate from `Excel` namespace): `associate`, `Invocation`, `CancelableInvocation`, `StreamingInvocation<T>`, `Error`. No COM descriptor counterpart (`None (Office.js-only)`; parallels XLL/VBA UDFs via `calc-func::UserFunctionContext` + `calc-host`'s async/RTD invocation path).

---

## 12. Events (All 57 `Excel` Events)

`Excel` defines **57** `on*` event properties (`OfficeExtension.EventHandlers<T>`) across 12 classes (plus `Office.EventType.DocumentSelectionChanged` / `BindingDataChanged` / `BindingSelectionChanged` in `Office` common API). `calc-host` today has **1** synchronous event source ([`crates/calc-host/src/events.rs`](../../crates/calc-host/src/events.rs): `WorkbookEvent::{Open, WorksheetChange, AfterCalculate, WorksheetCalculate, BeforeSave}` and `WorkbookEventObserver`), and `calc-com-host` delivers `AppEvents` over `IConnectionPoint`. **Zero** Office.js asynchronous event dispatchers exist today.

| Office.js Class.Event | Req Set | COM Descriptor Event (`AppEvents` / `WorkbookEvents` / `DocEvents`) | `calc-host` `events.rs` Today |
|---|---|---|---|
| `Workbook.onActivated` | 1.13 | `WorkbookEvents.Activate` / `AppEvents.WorkbookActivate` | `No` |
| `Workbook.onAutoSaveSettingChanged` | 1.9 | `None (Office.js-only)` | `No` |
| `Workbook.onSelectionChanged` | 1.2 | `WorkbookEvents.SheetSelectionChange` / `AppEvents.SheetSelectionChange` | `No` |
| `Worksheet.onActivated` / `WorksheetCollection.onActivated` | 1.7 | `DocEvents.Activate` / `WorkbookEvents.SheetActivate` / `AppEvents.SheetActivate` | `No` |
| `Worksheet.onDeactivated` / `WorksheetCollection.onDeactivated` | 1.7 | `DocEvents.Deactivate` / `WorkbookEvents.SheetDeactivate` / `AppEvents.SheetDeactivate` | `No` |
| `Worksheet.onCalculated` / `WorksheetCollection.onCalculated` | 1.8 | `DocEvents.Calculate` / `WorkbookEvents.SheetCalculate` / `AppEvents.SheetCalculate` | `Event-src` (`WorksheetCalculate` + `AfterCalculate`) |
| `Worksheet.onChanged` / `WorksheetCollection.onChanged` | 1.7 / 1.9 | `DocEvents.Change` / `WorkbookEvents.SheetChange` / `AppEvents.SheetChange` | `Event-src` (`WorksheetChange`) |
| `Worksheet.onSelectionChanged` / `WorksheetCollection.onSelectionChanged` | 1.7 / 1.9 | `DocEvents.SelectionChange` / `WorkbookEvents.SheetSelectionChange` / `AppEvents.SheetSelectionChange` | `No` |
| `WorksheetCollection.onAdded` | 1.7 | `WorkbookEvents.NewSheet` / `AppEvents.WorkbookNewSheet` | `No` |
| `WorksheetCollection.onDeleted` | 1.7 | `DocEvents.BeforeDelete` / `AppEvents.SheetBeforeDelete` (pre-delete in COM; post-delete in OJS) | `No` |
| `Worksheet.onColumnSorted` / `WorksheetCollection.onColumnSorted` | 1.10 | `None (Office.js-only)` | `No` |
| `Worksheet.onRowSorted` / `WorksheetCollection.onRowSorted` | 1.10 | `None (Office.js-only)` | `No` |
| `Worksheet.onFormatChanged` / `WorksheetCollection.onFormatChanged` | 1.9 | `None (Office.js-only)` | `No` |
| `Worksheet.onFormulaChanged` / `WorksheetCollection.onFormulaChanged` | 1.13 | `None (Office.js-only)` (COM fires `Change` for both) | `No` |
| `Worksheet.onNameChanged` / `WorksheetCollection.onNameChanged` | 1.17 | `None (Office.js-only)` | `No` |
| `Worksheet.onProtectionChanged` / `WorksheetCollection.onProtectionChanged` | 1.14 | `None (Office.js-only)` | `No` |
| `Worksheet.onRowHiddenChanged` / `WorksheetCollection.onRowHiddenChanged` | 1.11 | `None (Office.js-only)` | `No` |
| `Worksheet.onSingleClicked` / `WorksheetCollection.onSingleClicked` | 1.10 | `COM-absent` (`BeforeDoubleClick` / `BeforeRightClick` only in COM) | `No` |
| `Worksheet.onVisibilityChanged` / `WorksheetCollection.onVisibilityChanged` | 1.17 | `None (Office.js-only)` | `No` |
| `WorksheetCollection.onMoved` | 1.17 | `None (Office.js-only)` | `No` |
| `TableCollection.onAdded` / `onDeleted` | 1.9 | `None (Office.js-only)` | `No` |
| `TableCollection.onChanged` / `Table.onChanged` | 1.7 | `DocEvents.TableUpdate` / `AppEvents.SheetTableUpdate` (coarse table update) | `No` |
| `Table.onSelectionChanged` | 1.7 | `DocEvents.SelectionChange` (sheet-wide in COM) | `No` |
| `ChartCollection.onAdded` | 1.8 | `WorkbookEvents.NewChart` / `AppEvents.WorkbookNewChart` | `No` |
| `ChartCollection.onDeleted` / `onActivated` / `onDeactivated` | 1.8 | `COM-absent` (`_Chart.Activate`/`Deactivate` events exist in `ChartEvents`, not in descriptor) | `No` |
| `Chart.onActivated` / `Chart.onDeactivated` | 1.8 | `COM-absent` (`ChartEvents`) | `No` |
| `Shape.onActivated` / `Shape.onDeactivated` | 1.9 | `None (Office.js-only)` | `No` |
| `Binding.onDataChanged` / `Binding.onSelectionChanged` | 1.2 | `None (Office.js-only)` | `No` |
| `SettingCollection.onSettingsChanged` | 1.4 | `None (Office.js-only)` | `No` |
| `CommentCollection.onAdded` / `onChanged` / `onDeleted` | 1.12 | `None (Office.js-only)` | `No` |
| `LinkedEntityDataDomainCollection.onLinkedEntity*` / `onRefresh*` (×4) | 1.19 / 1.21 | `None (Office.js-only)` | `No` |

---

## 13. Design Implications for `dc-17spk.1` and `dc-1nbp.30`

1. **Same `ObjectModel` core, batching adapter on top**: The **136** Office.js domain members with COM-surface overlap (backed by `WorkbookAutomation`'s **206** workbook/report handlers) plus **366** `Excel.Functions` names present in `calc-catalogue` already cover a substantial slice of `ExcelApi 1.1`–`1.2` + `1.9` (`Range.values`, `formulas`, `text`, `address`, `getCell`, `getOffsetRange`, `getResizedRange`, `getEntireRow/Column`, `Worksheet.getRange`, `Workbook.worksheets`, `names`, `Table`, `Chart`, `Shape`, `Runtime.enableEvents`). An Office.js projection translates `RequestContext` command batches (`context.sync()`) into ordered `ObjectModel::invoke` calls without duplicating workbook semantics.
2. **Coordinate and value-shape differences live strictly in the Office.js adapter**:
   - `Range.rowIndex`, `Range.columnIndex`, `Worksheet.position`, `TableColumn.index`, `TableRow.index`, `getItemAt(i)` are **0-based** in Office.js vs **1-based** in COM/VBA.
   - `Range.values` / `formulas` / `text` always return 2-D `T[][]` arrays in Office.js (even for 1×1 cells), and `Range.values` uses raw primitives/serial numbers like COM `Range.Value2` (not `VT_DATE`/`VT_CY`).
   - `Range.address` includes the `'Sheet'!A1:B2` prefix in Office.js (`External:=False` in COM `Range.Address` omits the sheet name).
   - `Worksheet.tables` / `charts` / `shapes` map to `ListObjects` / `ChartObjects` / `Shapes`, with `Chart` combining COM `ChartObject` + `_Chart`, and `Shape.line` mapping to COM `ConnectorFormat`.
3. **Async vs synchronous events and leases**: COM uses synchronous `IConnectionPoint` callbacks and `IUnknown` refcounts; Office.js uses `context.trackedObjects` across async `context.sync()` boundaries and post-commit async `EventHandlers<T>` notifications. Exact batching/partial-failure/event ordering awaits the J1–J5 Windows probes in `dc-1nbp.30`.
4. **Guarded reach ([`CODE_EXECUTION_POLICY.md`](CODE_EXECUTION_POLICY.md) §1, §5.13)**: Office.js exposes no local filesystem or OS COM activation APIs (`ExcelApi`), so in Guarded mode the host mediates the Office.js projection (`ObjectModel`) and `fetch` (`network`), plus any external-link/query surfaces (`linkedWorkbooks`, `queries`, `dataConnections`, `hyperlink`).
