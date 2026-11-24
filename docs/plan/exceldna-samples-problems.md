# Excel-DNA/samples: collected problems

This document collects defects, stale metadata, broken builds, runtime bugs, and documentation mismatches found in [`Excel-DNA/samples`](https://github.com/Excel-DNA/samples) so the samples repository can be cleaned up later (owner approval required before any change to `Excel-DNA/samples`).

- **Inspected commit:** `ba5732c7d67d65fbc8610f714371f178aa0e5a8d` (read-only clone, 2026-11-24).
- **Phase covered so far:** Static inspection (`dc-z05nj.1`, Linux, no build). Build errors/warnings (`dc-z05nj.2`), Excel runtime findings (`dc-z05nj.3`), and DNA Calc runtime findings (`dc-z05nj.4`) will append to this list.

---

## Repo-wide cross-cutting patterns

These issues repeat across most of the repository and are summarized once here rather than repeated under every sample.

| Pattern | Count | Evidence | Suggested fix |
|---|---:|---|---|
| Floating prerelease package versions (`*-*`) | 48 of 52 projects | All SDK-style projects except `CustomTaskPane` (`1.7.0-rc9`), `RibbonFSharp` (`1.6.0`/`15.0.1`), and `RtdClock-ExcelRtdServer-PreRegistered` (`1.8.0`); also floats third-party packages (`System.Reactive`, `log4net`, `R.NET`, `System.Data.SQLite.Core`). | Pin to a single tested stable `ExcelDna.AddIn` release (or manage centrally via a root `Directory.Packages.props`), and pin third-party packages to explicit stable versions. |
| Package ID casing mismatch (`ExcelDna.Addin` vs `ExcelDna.AddIn`) | 41 vs 6 projects | 41 projects reference `ExcelDna.Addin` (lowercase `i`); 6 reference canonical `ExcelDna.AddIn` (`BatchedFunctionCalls`, `GetInfoFunctions`, `Registration.Sample`, `Registration.Sample.FSharp`, `Registration.Sample.VisualBasic`, `RibbonFSharp`). | Normalize all references to canonical NuGet ID `ExcelDna.AddIn` (matters on case-sensitive NuGet caches and package lock files). |
| End-of-life target frameworks | 47 of 52 projects | 42 projects target `net6.0-windows` (EOL Nov 2024); 4 target `net462` (`Registration/*`); 1 targets legacy `v4.5` (`Archive/Async/AsyncFunctions`). Only 2 target `net8.0-windows` (`AsyncThreadSafe`, `LosslessObservable`). | Update modern samples to `net472;net8.0-windows` (or `net8.0-windows` / `net9.0-windows` via a shared `Directory.Build.props`) and upgrade `net462` samples to `net472`. |
| Missing `README.md` | 14 of 40 non-Archive folders | `AsyncBatch`, `AsyncHelloWorld`, `BatchedFunctionCalls`, `ComServer`, `CustomTaskPane`, `DynamicCompiledVB`, `GenericHost`, `GetInfoFunctions`, `HttpClient`, `Misc`, `RtdClocksVB`, `RtdPerformance`, `SettingsProvider`, `Uploader`. | Add a short `README.md` per folder stating what the sample demonstrates, prerequisites, build steps, and how to test it in Excel. |
| Missing test workbook | 33 of 40 non-Archive folders | Only 7 folders ship a workbook (`ComServer`, `DnaComServer`, `GetInfoFunctions`, `ObjectHandles`, `Registration`, `RtdPerformance`, `Testing`). | Add a minimal `.xlsx` (or `.xlsm` where VBA is required) with expected formula/macro checks for each sample. |
| Legacy `.dna` runtime compilation | 61 `.dna` files (59 `Archive`, 2 `ArrayResizer`) | All `.dna` files rely on .NET Framework 4.x (or 2.0) on-the-fly CodeDOM compilation, which is unsupported on .NET 6/8. | Keep `Archive/` clearly marked as .NET Framework 4.x historical samples; migrate `ArrayResizer` to SDK-style C#/VB projects. |
| Unnecessary `System.Configuration.ConfigurationManager` in VB projects | 4 VB projects | `ComServerVB/ComAddin.vbproj:9`, `DynamicCompiledVB/DynamicCompiledVB.vbproj:14`, `RtdClocksVB/RtdClockVB-ExcelRtdServer/RtdClockVB-ExcelRtdServer.vbproj:9`, `UsingDAO/UsingDAO.vbproj:10` pull `7.0.0` only for default VB `My.Settings` templates. | Remove unused `My.Settings` boilerplate and the `System.Configuration.ConfigurationManager` reference unless the sample actually reads settings. |

---

## Problems grouped by sample

### `AddInReloader`
1. **Unclosed `FileStream` leak:** `AddInReloader/AddIn.cs:26` calls `configLoader.Deserialize(File.OpenRead(configPath))` without disposing the returned `FileStream`, keeping `AddInReloaderConfiguration.xml` locked.
   - *Suggested fix:* Wrap `File.OpenRead(configPath)` in a `using` block.
2. **NullReferenceException in `AutoClose()`:** `AddInReloader/AddIn.cs:37` calls `_watcher.Dispose()` unconditionally; if `AutoOpen()` failed to load the config (caught at `:29-32`), `_watcher` is `null`.
   - *Suggested fix:* Change to `_watcher?.Dispose();`.
3. **Watcher dictionary never populated (memory/handle leak):** `AddInReloader/AddInWatcher.cs:29` constructs `wd = new WatchedDirectory(directory, InvalidateAddIn)` without adding it to `_watchedDirectories`. Every subsequent watched add-in in the same directory creates a duplicate `FileSystemWatcher`, and `Dispose()` disposes nothing.
   - *Suggested fix:* Add `_watchedDirectories[directory] = wd;` right after line 29.
4. **Hardcoded developer path & missing copy-to-output:** `AddInReloader/AddInReloaderConfiguration.xml:2-3` hardcodes `C:\Work\ExcelDna\Samples\GeneralTestsCS\bin\Debug\GeneralTestsCS-AddIn.xll`, and `AddInReloader/AddInReloader.csproj` does not copy `AddInReloaderConfiguration.xml` to the output directory.
   - *Suggested fix:* Add `<None Update="AddInReloaderConfiguration.xml"><CopyToOutputDirectory>PreserveNewest</CopyToOutputDirectory></None>` and use a relative or documented sample path.

### `ArrayResizer`
1. **Unused stray parameters in C# UDF signature:** `ArrayResizer/CSharp/ArrayResizer.cs:58` defines `dnaMakeArrayAndResize(int rows, int columns, string unused, string unusedtoo)`, mismatching `ArrayResizer/VisualBasic/ArrayResizer.vb` and `ArrayResizer/README.md`.
   - *Suggested fix:* Remove `string unused, string unusedtoo` so C#, VB, and README match.
2. **No SDK-style project files:** `ArrayResizer/CSharp/ArrayResizer.dna` and `ArrayResizer/VisualBasic/ArrayResizer.dna` rely on legacy `.dna` runtime compilation only.
   - *Suggested fix:* Add SDK-style `.csproj` and `.vbproj` files targeting `net472;net8.0-windows`.

### `AsyncAwaitMacro`
1. **Syntax errors in README code snippets:** `AsyncAwaitMacro/README.md:33` and `:35` have unclosed string literals (`"abc;` and `"xyz;`).
   - *Suggested fix:* Close both string literals (`"abc";`, `"xyz";`).

### `AsyncBatch`
1. **Live plain-HTTP request to external site:** `AsyncBatch/AsyncBatch/AsyncBatchExample.cs:34` issues `httpClient.GetStringAsync("http://www.google.com")` (and discards the response), making tests depend on internet access and plain HTTP.
   - *Suggested fix:* Replace with a deterministic local `Task.Delay` or mock batch service so the sample works offline.
2. **Restricted to `net472`:** `AsyncBatch/AsyncBatch/AsyncBatch.csproj:4` targets only `net472`.
   - *Suggested fix:* Multi-target `net472;net8.0-windows`.

### `BatchedFunctionCalls`
1. **Nullable reference warning:** `BatchedFunctionCalls/FunctionParams.cs:5` declares `public string Ticker;` non-nullable without initializer or `required` (`CS8618`).
   - *Suggested fix:* Initialize to `string.Empty`, use a `record`, or mark nullable.
2. **Unawaited background task (`CS4014`) & double semicolon:** `BatchedFunctionCalls/Program.cs:17` starts `c.Reader.Batch(...).ReadAllAsync(...)` without discarding or observing exceptions; `BatchedFunctionCalls/Program.cs:35` has a stray double semicolon (`return await param.result.Task; ;`).
   - *Suggested fix:* Assign to `_ = ...` with error handling/logging and remove the extra `;`.

### `ComServer`
1. **Checked-in `TlbExp.exe` binary & `.NET Framework`-only TLB export:** `ComServer/ComServer.csproj:8-11` points `<ExcelAddInTlbExp>` to a checked-in `ComServer/TlbExp.exe`, which only works for `net472` assemblies and fails or is skipped for `net6.0-windows` (where `.NET` uses `dscom` / `dSPACE.Runtime.InteropServices.BuildTasks`).
   - *Suggested fix:* Remove the checked-in `TlbExp.exe` binary and use `dscom` / `ExcelDna.AddIn` built-in type-library generation conditioned properly per TFM, plus add a `README.md`.

### `ComServerVB`
1. **Missing `<ExcelAddInComServer>true</ExcelAddInComServer>`:** `ComServerVB/Class1.vb:21` calls `ComServer.DllRegisterServer()`, but `ComServerVB/ComAddin.vbproj` never sets `<ExcelAddInComServer>true</ExcelAddInComServer>`, so the generated `.dna` lacks `ComServer="true"` and COM registration will not expose the class.
   - *Suggested fix:* Add `<ExcelAddInComServer>true</ExcelAddInComServer>` to `ComServerVB/ComAddin.vbproj`.
2. **Inaccurate README:** `ComServerVB/Readme.md:3` states *"This sample shows the same functionality as ComServer, however in Visual Basic instead"*, but `ComServerVB` has no COM events (`[ComSourceInterfaces]`) and no test workbook (`Test.xlsm`).
   - *Suggested fix:* Either implement the matching COM events and workbook in `ComServerVB` or update `Readme.md` to describe what `ComServerVB` actually provides.

### `CustomTaskPane`
1. **Pinned stale prerelease package:** `CustomTaskPane/CustomTaskPane.csproj:9` pins `ExcelDna.Addin` to `1.7.0-rc9` (an old release candidate).
   - *Suggested fix:* Update to the current stable `ExcelDna.AddIn` version.
2. **Documented SDI multi-window bug unfixed:** `CustomTaskPane/CTPManager.cs:8` warns *"This won't work well under Excel 2013. There you need a different policy, since a CTP is attached only to a single window (one workbook)."* Modern Excel (2013+) is always SDI.
   - *Suggested fix:* Track task panes per `Window.Hwnd` (dictionary keyed by active window handle) instead of a single static `CustomTaskPane` field.

### `CustomXMLParts`
1. **Unhandled `InvalidCastException` on `InputBox` input:** `CustomXMLParts/MenuHandler.vb:109-114` checks `If DBModifType = vbNullString Then Exit Sub` and then immediately calls `Choose(CDbl(DBModifType), ...)`, which throws `InvalidCastException` if the user enters non-numeric text (or returns `Nothing` if out of range `1..3`).
   - *Suggested fix:* Validate with `Integer.TryParse` and range-check `1..3` before calling `Choose`.

### `DnaComServer`
1. **README contradicts `.csproj` and `AddIn.cs`:**
   - `DnaComServer/Readme.md:7,96` instructs installing `dSPACE.Runtime.InteropServices.BuildTasks`, whereas `DnaComServer/DnaComServer.csproj:9-11` uses a checked-in `TlbExp.exe` (which does not support `net6.0-windows`).
   - `DnaComServer/Readme.md:19,26,118,145,149` names the types `IComLibrary` / `ComLibrary` and `IComLibrary2` / `ComLibrary2`, whereas `DnaComServer/AddIn.cs:12,20,39,47` defines `IComLibraryC` / `ComLibraryC` and `IComLibrary2C` / `ComLibrary2C`.
   - `DnaComServer/Readme.md:141` says *"Put this code in to test late-bound COM Server"* right above `Sub TestEarlyBound()`.
   - *Suggested fix:* Align `AddIn.cs`, `DnaComServer.csproj`, `test.xlsm`, and `Readme.md` on one consistent set of class names and modern TLB tooling, remove checked-in `TlbExp.exe`, and fix line 141 to say *"early-bound"*.

### `DynamicCompiledVB`
1. **`PlatformNotSupportedException` on `net6.0-windows`:** `DynamicCompiledVB/AddIn.vb:33,47` uses `VBCodeProvider.CompileAssemblyFromSource`, which throws `PlatformNotSupportedException` on .NET 6/8 even though `DynamicCompiledVB.vbproj` multi-targets `net472;net6.0-windows`.
   - *Suggested fix:* Either restrict `DynamicCompiledVB.vbproj` to `net472` or switch compilation to Roslyn (`Microsoft.CodeAnalysis.VisualBasic`) for .NET 6/8+.
2. **Double registration & redundant package:** `DynamicCompiledVB/DynamicCompiledVB.vbproj` does not set `<ExcelAddInExplicitRegistration>true</ExcelAddInExplicitRegistration>`, causing `OldFunctions` to be registered both automatically and manually; it also lists `ExcelDna.Integration` explicitly even though `ExcelDna.AddIn` already brings it in.
   - *Suggested fix:* Set `<ExcelAddInExplicitRegistration>true</ExcelAddInExplicitRegistration>` and drop the redundant direct `ExcelDna.Integration` reference.

### `GenericHost`
1. **Work-in-progress commented registration & double registration:** `GenericHost/GenericHost.csproj:7-8` comments out `<ExcelAddInExplicitRegistration>` and `<ExcelAddInExplicitExports>`; `GenericHost/AddIn.cs:17-33` spawns a background `Task.Run` that registers functions once after 10 s and again after 20 s without documentation.
   - *Suggested fix:* Clarify the intended dynamic registration scenario, uncomment explicit registration if needed, and add a `README.md`.
2. **Checked-in Microsoft Office schema files:** `GenericHost/Resources/customUI.xsd` and `customui14.xsd` (6,468 lines total) are checked directly into the repo without clear usage or licence notice.
   - *Suggested fix:* Remove unused Microsoft XSD schemas from the sample folder (Visual Studio / XML tooling does not need them bundled in the add-in repo).

### `GetInfoFunctions`
1. **`IsMacroType = false` on XLM `xlfGetWorkbook` wrapper:** `GetInfoFunctions/GetInfoAddIn.cs:24` decorates `GetWorkbookActive` with `[ExcelFunction(..., IsMacroType = false)]`. In Excel, `xlfGetWorkbook` requires a macro-sheet equivalent (`IsMacroType = true`) and returns `#VALUE!` when `IsMacroType = false`.
   - *Suggested fix:* Change to `IsMacroType = true` (or explicitly document it as a negative test case in a new `README.md`).
2. **Unused `type_num` parameter:** `GetInfoFunctions/GetInfoAddIn.cs:37` defines `GetListSeparator(int type_num)` but ignores `type_num` and hardcodes `37` inside the body.
   - *Suggested fix:* Remove the unused `type_num` parameter (or rename the function if it is meant to forward `type_num`).

### `LimitedConcurrencyAsync`
1. **Outdated pre-SDK README & heading typo:** `LimitedConcurrencyAsync/README.md:1` omits the space after `#` (`#LimitedConcurrencyAsync sample`), and `:8` references obsolete Package Manager Console commands (`Uninstall-Package Excel-DNA` / `Install-Package Excel-DNA`) and hardcoded VS debug paths.
   - *Suggested fix:* Fix the heading markdown and update setup instructions for the SDK-style project.

### `LocalizedResources`
1. **Typos and outdated `.dna` packing instructions in README:** `LocalizedResources/Readme.md:22` misspells `Resource1.resx` as `Resource1.rex`; `:41-46` instructs editing a `.dna` file manually instead of SDK properties; `:56-57` ends with an unfinished `TODO:` referencing `v. 0.34`.
   - *Suggested fix:* Update `Readme.md` for SDK-style resource packing and remove the stale `v. 0.34` TODO.

### `Logging`
1. **Misleading XML comment & README filename mismatch:** `Logging/App.config:134` sits above the `VerboseLogging` configuration block but says `<!-- This sample configuration disables all logging -->`; `Logging/README.md:5` tells users to edit `Logging-AddIn.xll.config` while the project file to edit is `App.config`.
   - *Suggested fix:* Fix the comment on `App.config:134` and mention `App.config` (copied to `<name>-AddIn.xll.config` at build) in `README.md:5`.

### `LosslessObservable`
1. **Thread-safety races on `_observers` `List<IExcelObserver>`:**
   - `LosslessObservable/LosslessObservable/ObservableClock.cs:18,20,26-27` mutates `_observers` (`Add`/`Remove`) on the Excel/RTD thread while `OnTimerTick` iterates `_observers` on a `System.Threading.Timer` thread-pool thread without locking (`InvalidOperationException: Collection was modified`).
   - `LosslessObservable/LosslessObservable/ObservableTimedSequence.cs:19,20,25-26,32-33` has the exact same unsynchronized race.
   - *Suggested fix:* Protect `_observers` with a lock (or snapshot with `ToArray()` under a lock / `ConcurrentDictionary`) before iterating.
2. **Timer never disposed:** `LosslessObservable/LosslessObservable/ObservableClock.cs:12` starts `_timer` and never disposes it even when all observers unsubscribe.
   - *Suggested fix:* Dispose `_timer` when the last observer unsubscribes (or implement `IDisposable`).

### `MasterSlave`
1. **`TestController` builds a DLL instead of an EXE and mis-references `ExcelDna.Addin`:** `MasterSlave/TestController/TestController.csproj:3-5` omits `<OutputType>Exe</OutputType>` (even though `TestController.cs` defines `Main`), and line 8 adds an unnecessary `<PackageReference Include="ExcelDna.Addin" />` to a console controller app.
   - *Suggested fix:* Add `<OutputType>Exe</OutputType>` and remove `ExcelDna.Addin` from `TestController.csproj`.
2. **Hardcoded local path & wrong output path in `TestController`:** `MasterSlave/TestController/TestController.cs:35` hardcodes `C:\Work\Excel-DNA\Samples\MasterSlave\`, and `:46` loads `Master\bin\Debug\Master-AddIn64.xll` (missing the `\net472\` TFM subfolder produced by SDK-style builds).
   - *Suggested fix:* Resolve paths relative to `AppContext.BaseDirectory` including the TFM subfolder (and add `<ProjectReference>` to `Master` and `Slave`).
3. **Broken Ribbon callbacks in `Slave`:** `MasterSlave/Slave/SlaveRibbon.cs:33-34` wires `Button1` to `tag = 'ShowHelloMessage'` and `Button2` to `onAction = 'OnButtonPressed'`, neither of which exists in `Slave`.
   - *Suggested fix:* Implement `ShowHelloMessage` and `OnButtonPressed` in `Slave`, or align the XML with the actual handler methods.
4. **Excessive debug `Console.Beep()` calls & `net472`-only targeting:** `Master`, `Slave`, and `TestController` target only `net472` and emit repeated `Console.Beep()` calls during lifecycle hooks.
   - *Suggested fix:* Replace `Console.Beep()` with structured logging (`LogDisplay`/`Debug.WriteLine`) and multi-target `net472;net8.0-windows`.

### `Misc`
1. **VB syntax in C# file (`RegistrationInfoDump.cs`):** `Misc/RegistrationInfoDump.cs:45` (`wb.Sheets(1)`) and `:66` (`wb.Sheets(wb.Sheets.Count)`) use VB parentheses instead of C# indexer brackets (`wb.Sheets[1]`, `wb.Sheets[wb.Sheets.Count]`). Because `wb` is `dynamic`, this compiles only to fail at runtime (or fails under COM dispatch binding).
   - *Suggested fix:* Change to `wb.Sheets[1]` and `wb.Sheets[wb.Sheets.Count]`.
2. **Sheet name length crash:** `Misc/RegistrationInfoDump.cs:67` assigns `shInfo.Name = System.IO.Path.GetFileName(path)`, which throws a COM exception in Excel whenever the `.xll` filename exceeds 31 characters (or contains `[ ] : * ? / \`).
   - *Suggested fix:* Sanitize illegal sheet-name characters and truncate/deduplicate to 31 characters.
3. **Unpackaged standalone snippets:** `Misc/ComObjectModel.cs` and `Misc/RegistrationInfoDump.cs` have no `.csproj`, `.sln`, or `README.md`, so they are never compiled by any solution build.
   - *Suggested fix:* Wrap them in a small SDK-style project or move them into an existing sample with a `README.md`.

### `ObjectHandles`
1. **Typo & stray semicolon:** `ObjectHandles/ExcelFunctions.cs:16` returns `"Hello form ObjectHandles!"` (`form` -> `from`); `ObjectHandles/ObjectHandler.cs:53` has a double semicolon (`; ;`).
   - *Suggested fix:* Fix the typo and remove the extra semicolon.

### `Registration`
1. **`Registration.Sample.VisualBasic`: `<SignAssembly>true</SignAssembly>` without key file:** `Registration/Registration.Sample.VisualBasic/Registration.Sample.VisualBasic.vbproj:9` enables assembly signing without specifying `<AssemblyOriginatorKeyFile>`, causing build errors or warnings.
   - *Suggested fix:* Remove `<SignAssembly>true</SignAssembly>` (matching the C# and F# sibling projects).
2. **`Registration.Sample.FSharp`: stale `packages.config`, old `FSharp.Core`, obsolete `WebClient`:**
   - `Registration/Registration.Sample.FSharp/packages.config` (referencing `ExcelDna.AddIn 1.5.1` / `net452`) sits alongside the SDK-style `.fsproj` (`PackageReference`).
   - `Registration/Registration.Sample.FSharp/Registration.Sample.FSharp.fsproj:22-23` pins old `FSharp.Core` `4.7.2` and `System.ValueTuple` `4.3.0`.
   - `Registration/Registration.Sample.FSharp/AsyncFunctionExamples.fs:18,42` instantiates obsolete `WebClient` without `use` (undisposed).
   - *Suggested fix:* Delete `packages.config`, update `FSharp.Core`, remove redundant `System.ValueTuple` on modern TFMs, and switch `WebClient` to a shared `HttpClient`.
3. **`Registration.Sample`: `AsyncReturnHandler` string literal & missing attribute + README mismatch:**
   - `Registration/Registration.Sample/AsyncReturnHandler.cs:20` passes the literal string `"ExcelError.ExcelErrorGettingData"` to `[return: ExcelAsyncDefault(...)]` instead of `ExcelError.ExcelErrorGettingData`, and `:62` checks for `ExcelAsyncFunctionAttribute` while the sample method lacks it.
   - `Registration/Registration.Sample/README.md:6` names `AddIn.cs`, whereas the actual file is `ExampleAddIn.cs`.
   - *Suggested fix:* Pass the `ExcelError` enum (or fix the handler to map it), align the attribute on the sample function, and update `README.md:6` to `ExampleAddIn.cs`.

### `Ribbon`
1. **Code omits chart source-data call shown in README & Markdown typo:** `Ribbon/DataWriter.cs:42` creates and selects a chart (`ws.Shapes.AddChart(XlChartType.xlLineMarkers).Select();`) but omits `xlApp.ActiveChart.SetSourceData(Source: dataRange);` documented in `Ribbon/Readme.md:164`; `Ribbon/Readme.md:206` has an unclosed inline-code backtick.
   - *Suggested fix:* Add `SetSourceData` to `DataWriter.cs` (or update `Readme.md` to match) and close the backtick on line 206.

### `RibbonFSharp`
1. **Stale pinned packages & outdated README:** `RibbonFSharp/RibbonFSharp/RibbonFSharp.fsproj:14-16` pins old `ExcelDna.AddIn 1.6.0`, `ExcelDna.Integration 1.6.0`, and `ExcelDna.Interop 15.0.1`; `RibbonFSharp/README.md:8,14` instructs installing legacy `Excel-DNA` / `Excel-DNA.Interop` packages and references `Library1.fs` (actual files are `MyFunctions.fs` / `MyRibbon.fs`).
   - *Suggested fix:* Update package versions, drop redundant `ExcelDna.Integration`, and update `README.md` to match the current files.

### `RibbonVB`
1. **Broken 7-level relative GAC `HintPath`:** `RibbonVB/RibbonVB.vbproj:17-19` references `Microsoft.Vbe.Interop.Forms` with `<HintPath>..\..\..\..\..\..\..\Windows\assembly\GAC_MSIL\Microsoft.Vbe.Interop.Forms\11.0.0.0__71e9bce111e9429c\Microsoft.Vbe.Interop.Forms.dll</HintPath>`, which breaks on any checkout depth other than 7 levels below `C:\` and on machines without that exact GAC assembly.
   - *Suggested fix:* Replace with a NuGet interop reference (or standard `COMReference` / `EmbedInteropTypes`) and multi-target `net472;net8.0-windows`.

### `RtdArrayTest`
1. **Thread-safety race on `_topics`:** `RtdArrayTest/TestRtdServer.cs:51,59,65` mutates `_topics` (`Add`/`Remove`) on the Excel STA thread while iterating `_topics` on a `Timer` thread-pool callback without synchronization.
   - *Suggested fix:* Lock `_topics` on `Add`/`Remove` and snapshot it before iterating in the timer callback.

### `RtdClocks` & `RtdClocksVB`
1. **Thread-safety races on `_topics` / `_observers`:**
   - `RtdClocks/RtdClock-ExcelRtdServer/RtdClockServer.cs:34,40,46` (`_topics.Add`/`Remove`/`foreach` without lock).
   - `RtdClocks/RtdClock-IExcelObservable/ExcelObservableClock.cs:22,24,30` (`_observers.Add`/`Remove`/`foreach` without lock).
   - `RtdClocksVB/RtdClockVB-ExcelRtdServer/RtdClockServer.vb:30,35,41` (`_topics.Add`/`Remove`/`For Each` without `SyncLock`).
   - *Suggested fix:* Synchronize collection mutations and snapshot before iterating on timer ticks.
2. **Inconsistent pinning & checked-in user `launchSettings.json`:**
   - `RtdClocks/RtdClock-ExcelRtdServer-PreRegistered/RtdClock-ExcelRtdServer-PreRegistered.csproj` pins `ExcelDna.Addin 1.8.0` while all sibling projects float `*-*`.
   - Four subprojects (`RtdClock-ExcelRtdServer`, `RtdClock-IExcelObservable`, `RtdClock-Rx`, `RtdClock-Rx-Registration`) commit `Properties/launchSettings.json:5` with a hardcoded `C:\Program Files\Microsoft Office\root\Office16\EXCEL.EXE` path.
   - *Suggested fix:* Align package versions across `RtdClocks` and remove or `.gitignore` machine-specific `launchSettings.json`.
3. **Hardcoded `bin\Debug\net472` paths in test projects:** `RtdClocks/RtdClock-ExcelRtdServer.Test/ExcelRtdServerTests.cs:11` and `RtdClocks/RtdClock-ExcelRtdServer-PreRegistered.Test/ExcelRtdServerPreRegisteredTests.cs:11` hardcode `..\..\..\..\...\bin\Debug\net472\...` and target only `net472`.
   - *Suggested fix:* Reference the add-in build output via build configuration/TFM macros (or copy output via MSBuild target) so `Release` and `.NET 8` runs work.

### `RtdPerformance`
1. **Both projects compile empty DLLs (critical build bug):** `RtdPerformance/v032/RtdPerformance-v032.csproj:1-11` and `RtdPerformance/v110/RtdPerformance-v110.csproj:1-11` never include `..\Source\*.cs`, so `Source/AddIn.cs`, `DataService.cs`, `Functions.cs`, and `RtdServer.cs` are never compiled at all.
   - *Suggested fix:* Add `<Compile Include="..\Source\*.cs" />` to both `.csproj` files.
2. **Floating `*-*` defeats the `v032` vs `v110` benchmark:** Both `RtdPerformance-v032.csproj:8` and `RtdPerformance-v110.csproj:8` specify `Version="*-*"`, building the exact same latest prerelease version in both projects.
   - *Suggested fix:* Pin `v032` and `v110` to the intended Excel-DNA versions (or rename/retire `v032` if 0.32 is no longer supported).
3. **Missing `<ExcelAddInComServer>true</ExcelAddInComServer>`:** `RtdPerformance/Source/AddIn.cs:16` calls `ComServer.DllRegisterServer()`, but neither `.csproj` sets `<ExcelAddInComServer>true</ExcelAddInComServer>`.
   - *Suggested fix:* Add `<ExcelAddInComServer>true</ExcelAddInComServer>` to both projects.
4. **Duplicate assembly GUID:** `RtdPerformance/v032/Properties/AssemblyInfo.cs:23` and `RtdPerformance/v110/Properties/AssemblyInfo.cs:23` share the exact same GUID `e5ed525a-5168-4e0d-a7f3-89d56e833237`, causing COM registration collisions when both are tested.
   - *Suggested fix:* Assign distinct GUIDs to `v032` and `v110`.
5. **`Thread.Abort()` throws `PlatformNotSupportedException` on `net6.0-windows`:** `RtdPerformance/Source/DataService.cs:43` calls `_updateThread.Abort()`.
   - *Suggested fix:* Replace `Thread.Abort()` with cooperative `CancellationTokenSource` cancellation.

### `SettingsProvider`
1. **Every method throws `NotImplementedException` (broken at runtime):** `SettingsProvider/AddInSettingsProvider.cs:13-38` throws `NotImplementedException` in `ApplicationName`, `GetPropertyValues`, `Initialize`, and `SetPropertyValues`, while `SettingsProvider/Properties/Settings.settings:5` binds `UserName` to `ExcelDna.Samples.AddInSettingsProvider`. Clicking any ribbon button immediately crashes. Also contains an unused `Class1.cs`.
   - *Suggested fix:* Implement `AddInSettingsProvider` (e.g., reading/writing a JSON/XML file beside the XLL or in `%APPDATA%`), remove unused `Class1.cs`, and add a `README.md`.

### `Testing`
1. **Missing project reference & hardcoded `Debug` paths:** `Testing/TestingSample/Sample.Test/Sample.Test.csproj` has no `<ProjectReference>` to `../Sample/Sample.csproj` (so running `dotnet test` directly fails unless `Sample` was already built in `Debug`), and `Testing/TestingSample/Sample.Test/ExcelTests.cs:13,15` hardcode `bin\Debug\net472` and `bin\Debug\net6.0-windows`.
   - *Suggested fix:* Add a build dependency / `<ProjectReference>` (with `ReferenceOutputAssembly="false"`) on `Sample.csproj` and avoid hardcoding `Debug` in the test path.

### `Uploader`
1. **Off-by-one sheet bounds in `ExcelReference`:** `Uploader/UploadTracker.cs:65-66` constructs `new ExcelReference(0, ExcelDnaUtil.ExcelLimits.MaxRows, 0, ExcelDnaUtil.ExcelLimits.MaxColumns, ...)`. Because `ExcelReference` uses 0-based inclusive row/column indices, valid maximums are `MaxRows - 1` and `MaxColumns - 1`.
   - *Suggested fix:* Subtract `1` from `MaxRows` and `MaxColumns`.

### `UsingDAO`
1. **32-bit DAO incompatible with 64-bit Excel/DNA Calc & outdated README:** DAO/Jet 4.0 is 32-bit only (64-bit requires the Microsoft Access Database Engine / ACE provider); `UsingDAO/README.md:15-16,20` instructs installing legacy `Excel-DNA` / `Excel-DNA.Interop` packages and placing code in `AddIn.vb` (actual file is `Class1.vb`).
   - *Suggested fix:* Document the 64-bit ACE prerequisite and update `README.md` to match the SDK-style project and `Class1.vb`.

### `UsingLog4Net`
1. **README filename mismatch:** `UsingLog4Net/README.md:15-17` tells users to add `UsingLog4Net-AddIn.xll.config` with Copy to Output Directory, whereas the repo uses `App.config` (copied automatically by `ExcelDna.AddIn` MSBuild targets).
   - *Suggested fix:* Update `README.md` to match `App.config` and pin `log4net` instead of `*-*`.

### `UsingRDotNet`
1. **NullReferenceException when R is not installed & README typo:** `UsingRDotNet/AddIn.cs:40,46` dereferences `_engine` without checking whether `REngine.GetInstance()` succeeded; `UsingRDotNet/README.md:21` has a missing opening backtick (`=TestRDotNet()\``).
   - *Suggested fix:* Guard `_engine` with a clear error message when R is absent, pin `R.NET`, and fix the markdown typo.

### `UsingSQLite`
1. **Hardcoded DB path & dead download link:** `UsingSQLite/MyFunctions.cs:17` hardcodes `Data Source=C:\Temp\Northwind.db`; `UsingSQLite/README.md:9` links to shut-down `https://code.google.com/p/northwindextended/...`.
   - *Suggested fix:* Resolve the SQLite file relative to `ExcelDnaUtil.XllPath` (or ship a tiny self-creating SQLite DB on first open) and remove the dead Google Code link.

### `Archive` (59 `.dna` files + 1 legacy `.csproj`)
1. **`Archive/Async/AsyncFunctions` (broken build, runtime crash, defunct URLs):**
   - `Archive/Async/AsyncFunctions/AsyncFunctions.csproj:40` has a dead `<HintPath>..\..\..\ExcelDna.Integration.dll</HintPath>`, `:96` has a broken `<PostBuildEvent>` copying `..\..\..\ExcelDna.xll`, and the project uses legacy non-SDK `v4.5` + `packages.config`.
   - `Archive/Async/AsyncFunctions/AsyncAddIn.cs:46-47` calls `Log("TimerServer created")` *before* `_logPath` is initialized on line 47 (`ArgumentNullException` on `File.AppendAllText`), and `:65` reflects on a private Excel-DNA `TopicId` field.
   - `Archive/Async/AsyncFunctions/AsyncYahoo.cs:88,145` queries shut-down Yahoo Finance CSV endpoints (`http://download.finance.yahoo.com/d/quotes.csv`, `http://ichart.finance.yahoo.com/table.csv`).
   - *Suggested fix:* Mark as historical archive (or convert to SDK-style, fix `_logPath` init order, use public `Topic.TopicId`, and replace Yahoo URLs).
2. **`Archive/Async/HttpClientVB.vb` (defunct URL):** `Archive/Async/HttpClientVB.vb:53` scrapes shut-down `http://finance.yahoo.com/q?s=`.
   - *Suggested fix:* Mark archived or switch to a local/mock HTTP response.
3. **`Archive/RTD/TestRTD.dna` (ProgId mismatch):** `Archive/RTD/TestRTD.dna:29` calls `XlCall.RTD("MyRTDServers.TimeServer", null, "NOW")`, while `TimeServer` has no enclosing namespace and `[ProgId(...)]` on line 59 is commented out.
   - *Suggested fix:* Uncomment and align `[ProgId("MyRTDServers.TimeServer")]` with line 29.
4. **`Archive/RTD/ExcelRtdServer.dna` (hardcoded log path & unsynchronized `_topics`):** `Archive/RTD/ExcelRtdServer.dna:54` writes to hardcoded `C:\Temp\ExcelDnaRTD.log` and mutates `_topics` without locking.
   - *Suggested fix:* Write logs beside `ExcelDnaUtil.XllPath` (or `%TEMP%`) and lock `_topics`.
5. **`Archive/RTD/RTDObjectWrapper.dna` & `TestRTD2.dna` (duplicate & missing `RuntimeVersion`):** Both files are identical except line 1 (`RTDObjectWrapper.dna` has `RuntimeVersion="v4.0"`, while `TestRTD2.dna` omits it and defaults to obsolete CLR 2.0).
   - *Suggested fix:* Deduplicate or add `RuntimeVersion="v4.0"` to `TestRTD2.dna`.
6. **`Archive/Ribbon/*.dna` (filename case mismatch on `M.png`):** All 7 files (`CommandBars.dna`, `CommandBarsAllWays.dna`, `SharedGroup1.dna`, `SharedGroup2.dna`, `TestCSharp.dna`, `TestDynamic.dna`, `TestRibbon.dna`) declare `Path="M.png"`, whereas the file committed on disk is lowercase `Archive/Ribbon/m.png`.
   - *Suggested fix:* Change `Path="M.png"` to `Path="m.png"` (or rename the file) for consistency across case-sensitive tools.
7. **`Archive/ComServer/BuildSample.bat` & `Archive/Packing/PackAll.bat` (obsolete tool paths):** `Archive/ComServer/BuildSample.bat:1-9` hardcodes `.NET 2.0` (`v2.0.50727\csc.exe`), `Windows SDK v7.0A\bin\tlbExp.exe`, and missing `..\..\ExcelDna.Integration.dll` / `ExcelDnaPack.exe`. `Archive/Packing/PackAll.bat:1-34` likewise depends on missing `..\..\ExcelDna.Integration.dll` and `..\..\ExcelDnaPack`.
   - *Suggested fix:* Document `Archive/` as historical Excel-DNA 0.x/1.0 `.NET Framework` scripts that require manual setup, or migrate active scenarios to modern SDK-style samples.
8. **`Archive/Packing/PackConfig/PackConfig.xll.config` (root `C:\` log write):** `Archive/Packing/PackConfig/PackConfig.xll.config:10` sets `initializeData="C:\PackConfig.log"`, which fails with `UnauthorizedAccessException` for non-admin users on modern Windows.
   - *Suggested fix:* Point `initializeData` to a relative filename or `%TEMP%\PackConfig.log`.
9. **`Archive/CompilerV35.dna` (obsolete CLR 2.0):** `Archive/CompilerV35.dna:1` declares `RuntimeVersion="v2.0" CompilerVersion="v3.5"`.
   - *Suggested fix:* Keep clearly labeled as an obsolete .NET 2.0/3.5 archival test.

## Windows builds (2026-11-25, dc-z05nj.2)

Built upstream commit `ba5732c7d67d65fbc8610f714371f178aa0e5a8d` on
GOVERT-OMEN with .NET SDK 11.0.100. Each of the 94 project/framework pairs
was built once with the original project references and once with the
current stable Excel-DNA package versions pinned. NuGet resolved stable
1.9.0 for AddIn, Integration, IntelliSense, Registration,
Registration.VisualBasic and Testing. Interop and Interop.DAO were pinned to
16.0.0. The original sample clone was kept read-only. [NuGet AddIn versions](https://www.nuget.org/packages/ExcelDna.AddIn/1.9.0)

Both passes produced 87 successes and 7 failures. Each pass had 10
successful builds with warnings. The two Windows build columns in
[exceldna-samples.tsv](exceldna-samples.tsv) contain per-framework exits,
resolved package versions, warnings, errors and packed and unpacked XLL
paths. Build logs and XLL paths are relative to the ww17 run directory.

| Sample and framework | Observed in both passes |
|---|---|
| Archive/Async/AsyncFunctions, net45 | CS0234 and CS0246 for the legacy Excel-DNA and Reactive references. |
| ArrayMapCs, net472 and net6.0-windows | CS0138 at Functions.cs:4. The directive imports the XlCall type as a namespace. |
| ComServer and DnaComServer, net6.0-windows | Excel-DNA's typelib packaging task stopped when it tried to start dscom. It was absent from PATH on this host. |
| DynamicCompiledVB, net472 and net6.0-windows | BC36924 at RegistrationHelper.vb:12. The expression type uses the embedded interop Range type as a generic argument. The build stops before the previously reported dynamic compilation issue. |
