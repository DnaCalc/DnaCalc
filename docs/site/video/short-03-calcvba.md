# Short 3: CalcVBA — VBA running outside Excel (`dnacalc.com`)

- **Status:** Draft (`dc-6q2k.14`)
- **Target length:** 2–3 minutes (~2:35)
- **Audience:** Developers and organizations with `.xlsm` workbooks or `.xlam` add-ins who want to know how DNA Calc runs VBA outside Excel across platforms.
- **The one thing shown:** Running real workbook VBA outside Excel across **VM3** (portable bytecode interpreter) and **Cranelift JIT**, backed by a **host-neutral** Automation `VARIANT`/COM runtime and `ObjectModel` abstraction that scales from values-only Formula Explorer to full DNA Calc workbooks under explicit macro security.

## Exact files and setup

- **Workbook & module fixtures:** `crates/calc-xll-reference/fixture/vba/DnaSimple.xlsm` (`DnaSimple.bas`: `109` worksheet functions) and `crates/calc-vba/tests/corpus/` (`baseline.json`).
- **Apps / CLI:** `dna-calc-desktop.exe` (or `dnacalc diff crates/calc-xll-reference/fixture/vba/DnaSimple.xlsm` on Linux) and `/onecalc/` (`apps/dna-onecalc`).
- **Architecture & developer references:** `docs/articles/calcvba-variant.md`, `docs/site/pages/vba.html`, `crates/calc-vba/src/lib.rs`, `crates/calc-vba/src/projection.rs`, `crates/calc-vba/src/policy.rs`, `crates/calc-object-model/src/lib.rs`.

---

## Shot list and narration

### Shot 1 — Real VBA running outside Excel (0:00 – 0:35)

**On-screen actions**
1. Open `crates/calc-xll-reference/fixture/vba/DnaSimple.xlsm` in DNA Calc (with macro security initially `Off` / prompt).
2. Show the blocked-macro bar (`"Macros are disabled..."`), click **Enable macros**, and watch `=SimpleNeg(2.5)` → `-2.5`, `=SimpleSeq(3)` spill `10; 20; 30`, and `=SimpleWsMax(3, 9, 4)` → `9`.
3. Brief 3-second split to a Linux terminal running `dnacalc diff crates/calc-xll-reference/fixture/vba/DnaSimple.xlsm` (`109 of 109` matched).

**Narration**
> "A huge share of real Excel models depend on VBA—not just simple arithmetic, but `Variant` arrays, `Application.WorksheetFunction`, classes, and document events.
>
> Here is `DnaSimple.xlsm` opening in DNA Calc without Excel installed. Macros start disabled until you explicitly enable them. Once enabled, `SimpleNeg` evaluates, `SimpleSeq` spills a VBA `Variant` array onto the grid, and `SimpleWsMax` calls back into `WorksheetFunction.Max`—matching Excel on all `109 of 109` cases, on Windows and on the Linux runner."

---

### Shot 2 — One Automation runtime, two backends: VM3 and Cranelift JIT (0:35 – 1:15)

**On-screen actions**
1. Show `docs/articles/calcvba-variant.md` (heading *"One VARIANT, one COM object"* and the `VARIANT` / `SAFEARRAY` table) alongside `crates/calc-vba/Cargo.toml` (`portability = "portable"`) and `crates/calc-vba/src/lib.rs` (`prepare_project`).
2. Highlight the backend selection lines (`Backend::Jit` on native targets with clean pre-execution fallback to `Backend::Interpreter`, and `Backend::Interpreter` on `wasm32`).

**Narration**
> "Under the hood, CalcVBA is portable Rust with two in-process execution backends: **VM3**, our bytecode interpreter, and a **Cranelift JIT** compiler—plus AOT native builds.
>
> On Windows, Linux, and macOS, `calc-vba` compiles modules with the Cranelift JIT by default and falls back cleanly to VM3 before execution if a module needs an interpreter-only feature. In WebAssembly, VM3 runs directly in the browser.
>
> And both backends share the exact same memory layout: every `Variant` cell is the platform's real Automation `VARIANT` (`24` bytes on 64-bit, `16` bytes on 32-bit and `wasm32`), and every VBA class instance is a wrapper-free COM object—so `VarPtr`, `SAFEARRAY`s, `Implements`, and `WithEvents` behave identically across VM3 and the JIT."

---

### Shot 3 — Host-neutral by design: `ObjectModel`, `HostServices`, and `PolicyHost` (1:15 – 2:00)

**On-screen actions**
1. Show `crates/calc-object-model/src/lib.rs` (`Descriptor`, `ObjectModel`) and `crates/calc-vba/src/projection.rs` (`projection.rs`: translates CalcVBA `Variant` / `EarlyMemberCall` / `DynamicMemberCall` onto `calc_object_model::Call`).
2. Contrast the two profiles side by side:
   - **Formula Explorer** (`/onecalc/`, `Profile::ValuesOnly` / `InvocationModel`): `=Module1.Add2(A1,2)` → `42`, while an `As Range` UDF returns `"needs a workbook: open in DNA Calc"`.
   - **DNA Calc** (`Profile::Full` / `WorkbookAutomation`): live `Range` references, `Workbook_Open`, `Worksheet_Change`, `SheetCalculate`, `AfterCalculate`, and `PolicyHost` enforcing `Off` / `Guarded` / `Unguarded` macro security.

**Narration**
> "Crucially, CalcVBA itself is **host-neutral**: it has no Excel UI or workbook model hard-coded inside it.
>
> Instead, our `calc-vba` adapter projects CalcVBA's `HostServices` and `HostObjectServices` onto a single Rust `ObjectModel` trait driven by generated Excel type-library descriptors.
>
> In Formula Explorer's values-only profile, pure value UDFs run while `As Range` parameters return an explicit refusal: `needs a workbook: open in DNA Calc`. In full DNA Calc, the exact same VBA engine receives live `Range` objects and document events (`Workbook_Open`, `Worksheet_Change`, `AfterCalculate`), wrapped in `PolicyHost` so `Guarded` mode blocks file, process, `Declare`, and COM activation with standard VBA errors `70`, `53`, and `429`."

---

### Shot 4 — Corpus status and honest remainders (2:00 – 2:35)

**On-screen actions**
1. Open `/developers/vba/` (`docs/site/pages/vba.html`) showing the CalcVBA corpus baseline table (`crates/calc-vba/tests/corpus/baseline.json`).
2. Highlight the exact suites on both VM3 (`project/interpreter`) and JIT (`project/jit`): `dna-simple` (`109/109`), `vba-dictionary` (`15/15`), `vba-json` (`21/21`), `vba-regex` (`69/69`), `vba-arraytools` (`51/51`), `vba-stringbuilder` (`35/35`), `vba-web` (`45/45`), and `vba-test` (`31/31`), alongside the partial suites (`dna-examples`, `vba-keyedcollection`, `wasabi`).

**Narration**
> "We test both VM3 and the JIT side by side against Excel 16.0 on real third-party VBA libraries.
>
> In our committed baseline, `DnaSimple` (`109 of 109`), `VBA-Dictionary` (`15 of 15`), `VBA-JSON` (`21 of 21`), `VBA-Regex` (`69 of 69`), `VBA-ArrayTools` (`51 of 51`), `StringBuilder` (`35 of 35`), `VBA-Web` (`45 of 45`), and `VBA-Test` (`31 of 31`) all match Excel on both VM3 and the JIT. Where suites still have gaps—like `dna-examples` (`24 of 26`), `vba-keyedcollection` (`23 of 24`), or `wasabi` (`40 of 53` cross-platform, `41 of 53` on Windows)—we publish the exact counts and track every remainder in the open issue list."

---

## Facts relied on and sources

| Fact used in script | Exact source in repository |
|---|---|
| `DnaSimple.xlsm` (`SimpleNeg`, `SimpleSeq`, `SimpleWsMax`) matches `109 / 109` on module/VM3, JIT, and `.xlsm`, and `dnacalc diff` on Linux | `crates/calc-xll-reference/fixture/vba/DnaSimple.xlsm`, `crates/calc-vba/tests/corpus/baseline.json`, `docs/plan/RELEASES.md:114` |
| Portable Rust (`portability = "portable"`), three CalcVBA backends (VM3 interpreter, Cranelift JIT, AOT), `VARIANT` size `24` bytes (64-bit) / `16` bytes (32-bit/`wasm32`), wrapper-free COM objects (`dc-yqq0` `29 / 29` tasks) | `crates/calc-vba/Cargo.toml:9-10`, `docs/articles/calcvba-variant.md:3-18, 28, 57-66` |
| Native admission tries `Backend::Jit` and falls back to `Backend::Interpreter` (VM3) before execution on `UnsupportedBackend`/`BackendUnavailable`; `wasm32` uses `Backend::Interpreter` directly | `crates/calc-vba/src/lib.rs:159-178`, `crates/calc-vba/src/runtime/cache.rs:187-198` |
| Host-neutral `calc-object-model` (`Descriptor` from `crates/calc-object-model/descriptor/*.json`, `ObjectModel` trait) + `calc-vba` projection onto `HostServices` / `HostObjectServices` | `crates/calc-object-model/src/lib.rs:1-21`, `crates/calc-vba/src/projection.rs:1-9`, `crates/calc-vba/src/runtime/spreadsheet.rs:265-279, 502` |
| `Profile::ValuesOnly` (`InvocationModel`, Formula Explorer: `=Module1.Add2(A1,2)` → `42`, `As Range` → `"needs a workbook: open in DNA Calc"`) vs `Profile::Full` (`WorkbookAutomation`, DNA Calc workbook + `Workbook_Open`/`Worksheet_Change`/`SheetCalculate`/`AfterCalculate`) | `crates/calc-vba/src/lib.rs:66-78, 147-155`, `crates/calc-vba/src/invocation_model.rs:1-5`, `crates/calc-host/src/automation.rs:97, 1103`, `docs/site/pages/vba.html:7-9`, `docs/site/release-notes/next.md:43` |
| Macro security `Off`, `Guarded`, `Unguarded`; `PolicyHost` denies files, processes, environment, settings, `Declare`, COM activation, `SendKeys`/`AppActivate` with VBA errors `70`, `53`, `429` | `crates/calc-vba/src/policy.rs:1-10`, `docs/site/release-notes/next.md:36` |
| `baseline.json` project counts on both `project/interpreter` (VM3) and `project/jit` (JIT): `dna-simple` `109/109`, `vba-dictionary` `15/15`, `vba-json` `21/21`, `vba-regex` `69/69`, `vba-arraytools` `51/51`, `vba-stringbuilder` `35/35`, `vba-web` `45/45`, `vba-test` `31/31`, `dna-examples` `24/26` project / `22/26` `.xlsm`, `vba-keyedcollection` `23/24`, `wasabi` `40/53` common (`2` mismatched, `11` refused) / `41/53` Windows (`12` mismatched) | `crates/calc-vba/tests/corpus/baseline.json` |
| Remaining CalcVBA items (`dc-69djs` COM object identity follow-ups, `dc-q6brh.1` library named arguments, `dc-tey4.*` third-party project fixes) | `docs/site/release-notes/next.md:57` |
