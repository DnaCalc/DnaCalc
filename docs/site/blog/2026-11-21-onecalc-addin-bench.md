# Test an Excel-DNA add-in with Formula Explorer
Tags: Formula Explorer, Windows, add-ins, Excel-DNA
Status: draft

When you are building an Excel-DNA `.xll`, an xlOil add-in, or a `.xlam` function library, testing edge cases inside a full workbook can be clumsy. You want one formula, explicit inputs, immediate type and array visibility, and a clean way to restart if a native DLL misbehaves.

**DNA Calc - Formula Explorer** (`dna-onecalc`) is built for that job. In the browser at `onecalc.dnacalc.com` (and embedded on `dnacalc.com`), it runs standard Excel formulas in WebAssembly. On Windows x64 (`dna-onecalc-desktop.exe`), it adds a native add-in host so you can load `.xll` and `.xlam` add-ins directly—without starting Excel.

## Load an Excel-DNA add-in in the Add-ins panel

You can reproduce the walkthrough from Episode 2 using the reference fixtures in `crates/calc-xll-reference/fixture/`:

1. Build `X2Hello.csproj` (`Hello.cs`, exporting `SayHello`) and `X0.csproj` (`Probe.cs`, exporting `X0.Hello`, `X0.Reference`, `X0.Async`, `X0.Big`, and `X0.ThreadSafe`) in Release mode (`dotnet build -c Release`).
2. Open DNA Calc - Formula Explorer for Windows, expand **Add-ins**, and choose **Load add-in…** (or paste the path and click **Load path**).
3. Load `X2Hello-AddIn64.xll`. Formula Explorer records the file's SHA-256 hash, lists `SayHello` from `xlfRegister`, and feeds it into formula completion and signature help.

Enter `=SayHello("Excel-DNA")` and check the **Result** panel: it displays both the value type (`text`) and the value (`Hello, Excel-DNA`).

## Test references, async handles, and mixed arrays

Load `X0-AddIn64.xll` to exercise trickier C API boundaries:

- **Reference arguments:** Enter `=X0.Reference(A1)`. As soon as the formula names `A1`, the **Inputs** scratch grid (`Sheet1!A1:...`) opens automatically. Type `7` in `A1` and see `X0.Reference` read the `ExcelReference` directly; replace `7` with `"abc"` or an error to test coercion and error paths.
- **Asynchronous UDFs:** Enter `=X0.Async(41)`. `Probe.cs` queues a worker thread, sleeps 150 ms, and calls `ExcelAsyncHandle.SetResult(value + 1)`. Formula Explorer delivers `xlAsyncReturn` natively and updates the result to `42`.
- **Large mixed arrays:** Enter `=X0.Big()` to inspect a 1×4,096 mixed array (`"marker"`, `1`, `2`, ...) in the scrollable array viewer, or wrap it in `=SUM(X0.Big())` (`8386560`).

## Step inside a formula with F9 and Evaluation trace

Formula Explorer shares `dna-formula-ui` with DNA Calc's formula bar, giving you three fast inspection tools on any formula:

- **F1 function help** and live signature popups as you type.
- **F9 partial evaluation:** select any sub-expression in the formula field and press `F9` to replace it with its evaluated value; press `Escape` to restore the original text.
- **Evaluation trace:** click **Trace the formula** and use **Previous step** / **Next step** to walk through every sub-expression innermost-first, highlighted directly in the editor.
- **Exact value & Calculation details:** inspect the full binary64 representation alongside Excel's 15-digit display, raw result, provenance, date system, locale, and every cell read.

## Process isolation, sessions, and measured limits

Native add-ins run in a surrogate process, so the Add-ins panel includes **Stop add-in process** and **Restart add-in process** buttons if your add-in gets stuck while you iterate. When you save a session as an `.xlsx` (or `.xlsm` if you also added a VBA module), Formula Explorer records the paths and hashes of loaded add-ins and offers to reload them on reopen rather than executing anything automatically.

On the Windows cross-build acceptance suite (`docs/plan/WINDOWS_CROSS_BUILD.md`, second run):

- **Formula Explorer XLL cases (`m4`):** `350 of 350` matched Excel (`0` mismatched, `0` refused), covering .NET Framework 4.8, .NET 8, NativeAOT, xlw, and xlOil, plus `12 of 12` on `xloil-utils-sql`.
- **VBA add-ins (`m5`):** `109 of 109` each on `simple-xlam` and `simple-xlsm`, `22 of 22` on `dnaexamples-xlam`, and `23 of 23` on `dnaexamples-xlsm`.

Where Formula Explorer does not yet provide a host service—for instance, `X0` COM workbook properties still see `"Book1"` and workbook events such as `SheetChange` and `AfterCalculate` are still open under `dc-1nbp.3`—the host returns an explicit typed refusal rather than silent wrong behaviour. Keep the formula, inputs, and Excel build with any discrepancy you find so anyone can reproduce it.
