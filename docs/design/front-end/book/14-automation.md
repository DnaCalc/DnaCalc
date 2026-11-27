# Part XIV. Automation, Engine, and CLI

Clusters: `CL-14-01`–`CL-14-07` · Governing decisions: `D-04`, `D-88`, `D-89` · Architectural contract: [SKIN_ARCHITECTURE.md §§11, 13, 17.1](../SKIN_ARCHITECTURE.md) · Mockups: [Pro 21 G01](http://127.0.0.1:8720/p21/customise.html#G01), [Pro 21 G02](http://127.0.0.1:8720/p21/customise.html#G02), [Pro 21 G03](http://127.0.0.1:8720/p21/customise.html#G03), [Pro 21 E03](http://127.0.0.1:8720/p21/formula-explorer.html#E03)

---

## 14.1 Macro Security (`Off` / `Guarded` / `Unguarded`) and Honest Preflight (`CL-14-01`, `D-88`)

See [Pro 21 G01 — Guarded macro on Loan App](http://127.0.0.1:8720/p21/customise.html#G01) and [Pro 21 G03 — Blocked action refusal](http://127.0.0.1:8720/p21/customise.html#G03).

On workbook open, `calc-host` inventories VBA modules (`vbaProject.bin`), external workbook links (and target reachability), `RTD` topics, and `.xll`/`.xlam` UDF calls up front, holding macros and untrusted links inactive until the user decides (`C-2`) `[P19-CARBON-0208]`. Macro policy is governed strictly by the host (`calc-macro-security`)—**never** granted or elevated by an embedded skin, `dna.page/1` App, `#fragment` URL, or agent port—across **three explicit levels** (`D-88`, replacing earlier `'Safe'` wording so security names never overpromise) `[FRAMING-0159, P21-0115, D-88]`:

| Level | Host Policy | User Surface & Refusal Behaviour |
|---|---|---|
| **`Off`** | All VBA and `.xlam` macro execution refused. | Workbook opens and calculates all standard formulas normally; clicking a macro-bound App button or Form Control opens an inline **Blocked Macro Card** showing the procedure name, read-only VBA source text, and static call hints without running a single statement (`G03`, `Inherited.xlsm` `LegacyClose`) `[P21-0115, D-88]`. |
| **`Guarded`** | Workbook-scoped execution permitted **after scoped per-run or session trust review**; external I/O (`Shell`, filesystem writes, `CreateObject` network/COM, registry) and protected-cell writes are blocked by runtime host guards. | Clicking **`Stress Rate (12%)`** (`AppActions.StressRate` on `Loan.xlsm`, `G01`) displays a **Guarded Macro Review Card** with procedure source text and any declared workbook-write plan (`Loan!B3 -> 12%`), recalculates existing formulas on approval, and refuses blocked external/protected actions with a plain readout naming the exact statement and policy (`G03`) `[FRAMING-0159, P21-0115, P21-0116, D-88]`. |
| **`Unguarded`** | Full CalcVBA execution on native hosts that support it, strictly for explicitly user-trusted workbooks or verified enterprise signatures. | Runs desktop VBA including external COM/file automation where present on the host; **never** manufactures missing runtimes (such as Windows COM inside a browser build) `[P21-0115, D-88]`. |

### 14.1.1 Honest Static Inspection vs Arbitrary Dynamic VBA (`P21-0119`, `D-88`)

Pro 21 explicitly distinguishes finite declared action plans (`G01`) from arbitrary imperative VBA `[P21-0119]`. DNA Calc keeps that boundary scrupulously honest (`D-88`):

- **Upfront preflight rejection** applies when a host capability is statically absent (e.g., VBA execution in a browser build without CalcVBA, or `Off` policy) or when an App button invokes a finite declared `ModelChange` action plan targeting a protected cell.
- **Advisory static scan vs observed post-run truth:** `calc-host` lists VBA modules, procedures, bound controls/events, and object-model tokens without running code (`C-38`) `[P19-FABLE-0562]`, surfacing static range strings strictly as weak *advisory code hints*—**never** as formula dependency wires (`D-88`). During execution under `Guarded` mode, runtime capability checks block forbidden I/O calls at the call site without falsely claiming that arbitrary Turing-complete VBA was statically sandboxed or atomically rolled back before its first cell write `[P21-0119, D-88]`.

---

## 14.2 Excel Object Model Compatibility Across Every View (`CL-14-02`)

Legacy Excel VBA modules, COM automation clients (`crates/calc-com-host`), and add-ins operate directly on `HostSession` through the shared Rust object model (`crates/calc-object-model`, `docs/architecture/OBJECT_MODEL.md`), preserving Excel's semantics regardless of which view the user is looking at `[FRAMING-0016, P19-CARBON-0100, P19-FABLE-0204]`:

1. **Shared live UI members (`FRAMING-0016`):** `Application.Selection`, `Application.ActiveCell`, `ActiveWindow`, and `Application.Goto` read and move the session's shared `Selection` (`SelectionCause::Macro`, advancing past stale client sequence numbers cleanly across Classic, Inspector, Notebook, Map, and App) `[FRAMING-0016, P19-FABLE-0204]`.
2. **Dialogs and status text:** `MsgBox`, `InputBox`, and `UserForm` requests surface via `Event::Macro` over any active view; `Application.StatusBar = "..."` renders in a clearly badged **`Macro:`** slot inside the Strip (never overwriting core calculation/provenance truth badges) `[P19-FABLE-0204]`.
3. **`Application.ScreenUpdating = False` & interruption (`C-38`):** While `ScreenUpdating` is `False`, `calc-skin-host` withholds intermediate tile and watch `Frame` broadcasts so 10,000-write loops batch into a single recalculate-and-paint frame at `end_run`, **while keeping the Strip's `'Macro running (<proc>) · Esc interrupts'` badge live and responsive** (`C-38`) `[P19-CARBON-0100, P19-FABLE-0204, P19-FABLE-0562]`.

---

## 14.3 The Three-Verb Late-Bound VBA Views API (`CL-14-03`, `D-89`)

See [Pro 21 G02 — Macro switches view & selects block](http://127.0.0.1:8720/p21/customise.html#G02).

To let workbook authors pair macros with non-Classic views (`ReviewRepaymentCells` switching to Classic or App and highlighting `Loan!B2:B6`) without breaking Excel compatibility, DNA Calc exposes **strictly three presentation verbs** in v1 over a single host dispatcher (`D-89`, `R-65`, `R-99`, `C-39`) `[FRAMING-0017, FRAMING-0128, P19-FABLE-0516, P19-FABLE-0563, P19-GPT-0295, D-89]`:

```vb
' Portable late-bound Application.Run shims (recommended: compiles in Excel with zero references)
On Error Resume Next
Application.Run "DnaCalc.View.Switch", "classic", "Loan"
Application.Run "DnaCalc.View.Select", "Loan!B2:B6"
Application.Run "DnaCalc.View.ShowPanel", "inspector.trace", True
On Error GoTo 0

' Equivalent late-bound COM helper on Windows desktop (CreateObject("DnaCalc.Views"))
Dim views As Object
On Error Resume Next
Set views = CreateObject("DnaCalc.Views")
If Not views Is Nothing Then
    views.TryShow "classic", "Loan"
    views.Select "Loan!B2:B6"
    views.ShowPanel "inspector.trace", True
End If
On Error GoTo 0
```

*(Short aliases `"DnaCalc.View"`, `"DnaCalc.Select"`, and `"DnaCalc.Panel"` route to the same three handlers `[P19-CARBON-0101, P19-FABLE-0205, P21-0117, D-89]`.)*

- **Seamless Excel degradation (`R-65`, `R-99`):** Calculation macros never depend on DNA-only view calls to compute right answers in Excel; in desktop Excel, `On Error Resume Next` steps harmlessly past the three presentation verbs while native `Worksheets("Loan").Range("B2:B6").Select` still selects the same cells `[P19-CARBON-0101, P19-FABLE-0205, P19-FABLE-0516, P19-GPT-0078, P19-GPT-0295, P21-0117]`.
- **Non-negotiable guardrails:** View and panel requests queue until any active user cell/formula edit commits, record in navigation (`Alt+Left` steps back) **without consuming a workbook Undo step**, and **can never** dismiss a Trust Card, suppress Strip security/error/provenance readouts, change `Application.Calculation` silently for a view, bypass App formula-cell protection, or install/change a skin `[P19-CARBON-0101, P19-FABLE-0206, P19-GPT-0078, P21-0117]`.

---

## 14.4 Undo Across Macro Runs: v1 Grouping & Checkpoints vs M8 `DocumentOnly` Design (`CL-14-04`, `D-04`)

Owner Ruling #4 (`D-04`) directs DNA Calc to design the architecture for Undo across a macro run, treat its subtleties with complete honesty, and **not promise general macro `Ctrl+Z` in the first release** `[FRAMING-0169, D-04]`.

1. **First-release commitment (Excel parity + pre-run checkpoint, `R-66`, `C-25`):**
   - `HostSession::begin_run` / `end_run` groups all observed cell/structure mutations of a macro run into a single `HistoryEntry` (`Origin::Macro`, `Reversibility::Irreversible`) naming the procedure and observed changed regions (`C-25`) `[P19-CARBON-0100, P19-GPT-0079, P19-GPT-0324]`.
   - Standard `Application.OnUndo` and `Application.Undo` are supported for macros that register custom reversal routines (`SKIN_ARCHITECTURE.md §17.1`).
   - Before any write-capable macro or provider action executes, `calc-host` identifies when `Ctrl+Z` will not reverse external side effects (`R-66`), pins a zero-copy `ModelRoot::pin()` **pre-run workbook checkpoint** (`Arc`-shared `CellPage`s), and exposes one-click **`Restore workbook before <MacroName>`**—explicitly distinguishing full document restoration from irreversible external side effects (`WinHTTP`, file writes, clipboard) `[P19-GPT-0079, P19-GPT-0296]`.
   - Finite `Guarded` action-plan macros (`G01`: `AppActions.StressRate` writing `Loan!B3 -> 12%`) execute as standard attributable workbook `Batch`es and undo cleanly in one `Ctrl+Z` step `[P21-0116]`.
2. **Milestone 8 experimental `Reversibility::DocumentOnly` (`SKIN_ARCHITECTURE.md §17.1`):** Before enabling direct `Ctrl+Z` across general imperative VBA runs, M8 measures all seven hard interactions documented in `SKIN_ARCHITECTURE.md §17.1`: surviving VBA module/`Static` globals, un-rolled-back external I/O, `Worksheet_Change` re-entrancy on Windows Excel, multi-workbook `Application.*` state, `Application.OnUndo` precedence, mid-run faults/`Esc` interrupts, and `command::restore` dependency-replan latency on 300k-formula workbooks.

---

## 14.5 Add-In Host, External Links, RTD Feeds, and Cross-Platform Honesty (`CL-14-05`–`CL-14-07`)

See [Pro 21 E03 — Explorer: add-in library](http://127.0.0.1:8720/p21/formula-explorer.html#E03).

1. **Shadow-copied, crash-isolated Add-In Host (`CL-14-05`, `C-18`, `C-23`, `C-37`):** `crates/calc-addin-host` and its out-of-process surrogate (`calc-surrogate-wire`) load, unload, and **hot-reload** `.xll` and `.xlam` packages from shadow copies **without locking build outputs on disk** (`Ade`'s `cargo build` / C++ rebuild workflow), enumerate exported UDF signatures, argument help, thread-safety (`$` suffix), and volatility (`!`), time individual calls, surface rich per-argument diagnostics, tag dependent cells with add-in provenance, write portable `_xll.` prefixes + cached values in `.xlsx`, and survive add-in faults without crashing DNA Calc `[P19-CARBON-0224, P19-FABLE-0561, P19-NEON-0261]`.
2. **External Links, Volatiles, and Live RTD Telemetry (`CL-14-06`, `C-21`, `C-22`, `C-35`, `C-36`):** `WorkbookQuery::Sources` inventories every external workbook link (`Resolved` vs `Missing` like `Rates2024.xlsx`, saved cache values, last update time, and full dependent cell closure), flags volatile built-ins vs add-in UDFs, and streams `RTD` / async topic updates (`server`, `topic`, timestamp, observed Hz, and UI-side display coalescing/hold that pauses visual churn without altering formulas or falsely claiming to stop the upstream RTD server) `[P19-FABLE-0559, P19-FABLE-0560, P19-NEON-0259, P19-NEON-0260]`.
3. **Cross-Platform Host Boundaries & Prototype Honesty (`CL-14-07`):** Native Windows `.xll` binaries and COM `IRtdServer`s require the Windows desktop host (`dna-calc-desktop` / `dna-onecalc-desktop`). On WebAssembly browser builds and Linux/macOS `dnacalc serve`, unsupported native providers never fail silently or show `#NAME?` without context: DNA Calc displays the file's cached values badged **`◊ As saved · Requires Windows desktop add-in (<name>)`** with a one-click **`Open in desktop app`** (`dnacalc://open#...`) action `[P19-FABLE-0231, P19-FABLE-0235]`. Similarly, Pro 21's mockups simulated VBA, `.xll`, and RTD in JavaScript (`P21-0126`); native runtime parity is verified through the Rust crates (`calc-addin-host`, `calc-com-host`, CalcVBA) and Windows Excel acceptance suites `[P21-0126]`.

---

## 14.6 Headless CLI (`dnacalc`), Rust Engine Embedding, and Agentic Automation

As foregrounded on `dnacalc.com` (Part I §1.2.3), DNA Calc's UI views sit atop the exact same portable engine and host crates available to scripts, CI pipelines, Rust services, and AI agents with zero UI or browser overhead (`SKIN_ARCHITECTURE.md §§11, 13`):

| Entrypoint | Crate / Binary | Capabilities |
|---|---|---|
| **Headless CLI** | `apps/dnacalc` | `dnacalc recalc <in.xlsx> [-o out.xlsx]` (full Excel-parity recalculation), `dnacalc get <file.xlsx> <SourceRef> [--json]`, `dnacalc set <file.xlsx> <cell=expr>...`, `dnacalc diff <a.xlsx> <b.xlsx>` (value, formula, and `urn:dnacalc:views:v1` structural diff), `dnacalc run <file.xlsm> <Macro>` (under `--macro-policy guarded|unguarded`), `dnacalc skin validate <skin.json> [--workbook file.xlsx]` (sub-second schema, 61-pair sRGB contrast, keymap departure, and anchor check), and `dnacalc serve <file.xlsx>` (launches local browser UI over loopback WebSocket on Linux, macOS, or Windows). |
| **Rust Engine & Formula Crates** | `calc-value`, `calc-func`, `calc-fml`, `calc-doc`, `calc-engine`, `calc-formula-host`, `calc-host`, `calc-skin` | Embeddable MIT Rust crates (`wasm32-unknown-unknown` portable from `calc-value` through `calc-skin`): run standalone formula evaluation/formatting (`calc-formula-host`), full workbook recalculation (`calc-host`), or drive neutral `WorkbookQuery` / `Batch` / `Proposal` workflows programmatically. |
| **Windows COM Server** | `crates/calc-com-host` | Projects the one Rust object model (`calc-object-model`) to external Python (`win32com`), PowerShell, C#, and C++ automation scripts on Windows. |
| **Agent Port (`calc-skin`)** | Loopback MCP / `SkinHost` | Auto-generates tool schemas from `calc-skin`'s `Query` and `CommandTable`, enforcing `WritePolicy::Propose` for model batches (`ProposalUnit` diffs + `<trail>`) and `Operation::TryOn` for presentation/skin JSON. |
