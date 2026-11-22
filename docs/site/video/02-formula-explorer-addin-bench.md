# Episode 2: Formula Explorer as a test bench for an Excel-DNA add-in

- **Status:** Draft (`dc-6q2k.4`)
- **Target length:** 7–8 minutes
- **Audience:** Excel-DNA, xlOil, C API, and `.xlam` add-in authors who want a fast, inspectable bench outside Excel for testing UDFs, argument types, async calls, and edge cases.
- **The one thing shown:** Loading a real Excel-DNA `.xll` (and a `.xlam` / inline VBA module) in **DNA Calc - Formula Explorer**, inspecting arguments and intermediate steps with `F1`, `F9`, and **Evaluation trace**, reading exact binary64 / type output, and seeing how unmodelled host calls fail loudly with a typed refusal.
- **Paired blog post:** [`docs/site/blog/2026-11-21-onecalc-addin-bench.md`](../blog/2026-11-21-onecalc-addin-bench.md)

## Exact files and setup

Prepare these repository fixtures on the Windows recording machine:

1. **Application binary:** `dna-onecalc-desktop.exe` (Windows x64 desktop build of DNA Calc - Formula Explorer), plus a browser tab open to `onecalc.dnacalc.com` (or local site preview) for the quick browser comparison at the start.
2. **Excel-DNA Add-in 1 (minimal):** `X2Hello-AddIn64.xll` built from `crates/calc-xll-reference/fixture/X2Hello.csproj` (`Hello.cs`: `SayHello(string name)`).
3. **Excel-DNA Add-in 2 (rich probe):** `X0-AddIn64.xll` built from `crates/calc-xll-reference/fixture/X0.csproj` (`Probe.cs`), with test formulas from `crates/calc-xll-reference/fixture/x2-cases.json`:
   - `=X0.Hello("world")` -> `"Hello, world"`
   - `=X0.Reference(A1)` -> reads `A1` via `ExcelReference` (put `7` in `A1`)
   - `=X0.Async(41)` -> asynchronous `ExcelAsyncHandle.SetResult(value + 1)` -> `42`
   - `=SUM(X0.Big())` -> 4,096-element mixed array (`8386560`, because element 0 is `"marker"` and elements `1..4095` are numbers `1..4095`)
   - `=X0.ThreadSafe(21)` -> `42`
   - `=X0.Caller()` -> `xlfCaller` `ExcelReference` description
4. **VBA Add-in (optional contrast):** `crates/calc-xll-reference/fixture/vba/DnaExamples.xlam` (`=DnaAdd(40, 2)` and `=DnaWhere(A1:B3)`).

Build command for the XLL fixtures on Windows:
```powershell
dotnet build -c Release crates/calc-xll-reference/fixture/X2Hello.csproj
dotnet build -c Release crates/calc-xll-reference/fixture/X0.csproj
```

---

## Script

### Scene 1 — Why Formula Explorer exists (0:00 – 0:55)

**Screen**
- Start on the browser version of **DNA Calc - Formula Explorer** (`onecalc.dnacalc.com` or the embedded Formula Explorer on the home page).
- Click **Try SUM with two inputs** (or open `Examples` -> `SUM & blank input`), then switch to `dna-onecalc-desktop.exe` on Windows.

**Narration**
> "When you're debugging an Excel formula—or an Excel-DNA add-in function—opening a whole workbook and hunting across sheets is slow.
>
> DNA Calc - Formula Explorer is our formula-only workbench. In the browser it runs pure Excel formulas in WebAssembly with no installation. On Windows, the desktop build adds a native x64 add-in host so you can load `.xll` and `.xlam` add-ins directly—without starting Excel."

---

### Scene 2 — Loading an Excel-DNA XLL in the Add-ins panel (0:55 – 2:20)

**Screen**
1. In `dna-onecalc-desktop.exe`, click **Add-ins · 0 loaded** to expand the Add-ins section.
2. Point briefly at the explanation text and the **Load add-in…** / **Load path** / **Stop add-in process** controls.
3. Click **Load add-in…** (or paste the path to `X2Hello-AddIn64.xll` and click **Load path**).
4. Show the loaded entry appear in the list with its path, file hash, and registered function `SayHello`.
5. Click into the formula field, type `=Say`, show the completion popup and signature help, and finish `=SayHello("Excel-DNA")`.
6. Point to the **Result** panel: type badge `text` and value `"Hello, Excel-DNA"`.

**Narration**
> "Let's load a minimal Excel-DNA add-in first: `X2Hello-AddIn64.xll` from `crates/calc-xll-reference/fixture/X2Hello.csproj`.
>
> I'll expand `Add-ins`, load the `.xll`, and you can see Formula Explorer immediately lists its SHA-256 hash and the worksheet functions it registered via `xlfRegister`—here, `SayHello`.
>
> When I start typing `=SayHello` in the formula bar, completion and signature help already know about it. I pass `"Excel-DNA"`, and the Result panel gives me both the Excel type (`text`) and the value (`Hello, Excel-DNA`)."

---

### Scene 3 — References, async handles, and arrays with `X0-AddIn64.xll` (2:20 – 4:40)

**Screen**
1. In **Add-ins**, load `X0-AddIn64.xll` (built from `X0.csproj` / `Probe.cs`).
2. Type `=X0.Reference(A1)` into the formula field.
3. Point out that the **Inputs** panel (`Inputs · no values entered`) opens automatically as soon as the formula names `A1`.
4. Click `A1` in the `Sheet1` scratch grid and type `7`. Watch `=X0.Reference(A1)` return `7`. Change `A1` to `"abc"` and show the result change to `text` `"abc"`, then back to `7`.
5. Type `=X0.Async(41)`: show the brief calculating state while the background thread sleeps 150 ms and calls `ExcelAsyncHandle.SetResult(42)`, then show `42` appear via `xlAsyncReturn`.
6. Type `=X0.Big()`: show the **Array result** panel scrolling through 4,096 columns (`"marker"`, `1`, `2`, ...), then wrap it in `=SUM(X0.Big())` -> `8386560`.

**Narration**
> "Now let's load a much richer Excel-DNA test add-in: `X0-AddIn64.xll` from `Probe.cs`.
>
> First, let's test a macro-type function that takes an `ExcelReference` rather than a coerced value: `=X0.Reference(A1)`. As soon as my formula mentions `A1`, the `Inputs` scratch grid opens on the right so I can supply cell `A1`. Let's put `7` in `A1`, and `X0.Reference` reads the `ExcelReference` directly through the C API callback.
>
> Next, `=X0.Async(41)`. In `Probe.cs`, that queues a worker thread, sleeps 150 milliseconds, and calls `ExcelAsyncHandle.SetResult`. Formula Explorer handles `xlAsyncReturn` natively and updates the result to `42` when the callback fires.
>
> And what about large mixed arrays? `=X0.Big()` returns a 1-by-4,096 array starting with the string `"marker"` followed by numbers `1` to `4095`. Formula Explorer renders the spilled array in a scrollable view, and if I wrap it in `=SUM(X0.Big())`..."

---

### Scene 4 — Inspecting parts: `F1` help, `F9` partial eval, Evaluation trace, and `Exact value` (4:40 – 6:10)

**Screen**
1. Put `10` in `A1` and `20` in `A2`, or use `=SUM(X0.Big()) + X0.ThreadSafe(A1)`.
2. Select `X0.ThreadSafe(A1)` inside the formula bar and press **F9**: show the selected sub-expression replaced by its evaluated value (`20` if `A1=10`), then press **Escape** to restore the formula text.
3. Click **Trace the formula** in the **Evaluation trace** panel: click **Next step** / **Previous step** to walk innermost-first through `X0.Big()`, `SUM(X0.Big())`, `X0.ThreadSafe(A1)`, and the final `+`.
4. Also try `=0.1+0.2` and expand **Exact value** under Result (showing the exact decimal value vs the 15-digit Excel display `0.3`), and expand **Calculation details** to show **Raw result**, **Provenance**, and **Reads**.

**Narration**
> "When a formula behaves unexpectedly, Formula Explorer gives you Excel's own inspection tools plus a few extras.
>
> Highlight any part of the formula and press `F9` to evaluate just that sub-expression in place—then press `Escape` to restore the original formula. Or click `Trace the formula` under `Evaluation trace` and step through every sub-expression from the inside out, highlighted in the editor as you go.
>
> If you're checking floating-point edge cases, `Exact value` shows the exact binary64 value alongside Excel's 15-digit display, and `Calculation details` logs every cell read and the active date system and locale."

---

### Scene 5 — Saving a reproducible session and understanding typed refusals (6:10 – 7:20)

**Screen**
1. Expand **Session file** and click **Save as…** (`.xlsx`), noting how the saved file records the loaded add-in paths and SHA-256 hashes without auto-loading executable code on reopen.
2. Also show what happens when an add-in hits something not yet modelled—or show the browser Formula Explorer panel reading `"Add-ins load in the desktop app; this page runs without a host that can load them"`—and mention **Stop add-in process** / **Restart add-in process** in the Add-ins panel for recovering from a misbehaving add-in.
3. Briefly mention the measured Windows cross-build numbers: `350 of 350` on the Formula Explorer XLL cases (`m4`), `12 of 12` on `xloil-utils-sql`, and `109 of 109` on `DnaSimple` `.xlam` / `.xlsm` (`m5`).

**Narration**
> "A few practical notes if you use this as your add-in test bench.
>
> First, the native add-in runs in a surrogate process. If a build crashes or locks up while you're iterating, you can click `Stop add-in process` or `Restart add-in process` right here without losing your formula or inputs.
>
> Second, saving a session writes an ordinary `.xlsx` (or `.xlsm` with VBA) that records which add-ins were loaded and their hashes, and offers to reload them when reopened rather than running anything silently.
>
> Third, we don't fake missing host services. On our Windows cross-build acceptance suite, Formula Explorer matched `350 of 350` XLL cases—including .NET Framework, .NET 8, NativeAOT, xlOil, and xlw fixtures—and `12 of 12` on `xloil-utils-sql`. Where a workbook event or COM member isn't implemented yet, such as `SheetChange` or real workbook names instead of `Book1`, DNA Calc returns a typed refusal naming what's missing so you can tell a host gap from a bug in your add-in.
>
> Links to all the fixture files are in the paired blog post."

---

## Paired blog post outline (`docs/site/blog/2026-11-21-onecalc-addin-bench.md`)

1. **Why a formula workbench for add-ins:** Testing one function and its inputs without spinning up a full workbook.
2. **Walking through `X2Hello` and `X0`:** Registration (`xlfRegister`), `ExcelReference` inputs, `ExcelAsyncHandle` (`xlAsyncReturn`), and mixed 4,096-element arrays.
3. **Inspecting formulas:** `F1` help, `F9` partial evaluation, `Evaluation trace`, `Exact value`, `Inputs`, and `Calculation details`.
4. **Process isolation, saved sessions, and honest limits:** Surrogate restart, `.xlsx`/`.xlsm` session files with file hashes, `350 of 350` (`m4`) and `12 of 12` (`xloil-utils-sql`) measured results, and the open `"Book1"` / events gap (`dc-1nbp.3`).
