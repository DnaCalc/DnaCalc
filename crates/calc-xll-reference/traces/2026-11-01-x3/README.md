# X3 measurements, 2026-11-01

Identical add-in files run in Excel 16.0.20527.20088 x64 and the DNA Calc desktop
host, using X0's debugger method and independent controls without the debugger.
The final host captures use source `ed838379`; each environment records the
executable, shim, tracer, add-in, configuration, case and source hashes.

| Profile | Formulas | Native calls | DLLFree returns | Pre-close callbacks | Raw differences |
|---|---:|---:|---:|---:|---:|
| Original xlw sample | 5 | 5 | 0 | 12 | 0 |
| xlw scalar/matrix/reference fixture | 5 | 5 | 0 | 14 | 0 |
| Original xlOil native plugins | 12 | 13 | 12 | 78 | 3 |
| Original xlOil Python plugin | 5 | 5 | 5 | 87 | 12 |

The original xlw 23.10.12-rc generator/library build the unchanged upstream sample
and a separate MIT fixture. Original xlOil 0.22.1 binaries run native Utils/SQL
and Python 3.12 profiles with documented INI settings and a small MIT Python
fixture. The framework binaries are unchanged. Versions, release/source URLs,
licences and captured runtime hashes are in `../../fixture/x3-upstream.json`.
Rebuilding xlw can change PE timestamps; compare one build's identical files in
both hosts. Binary inputs remain outside repositories under
`C:/Work/DnaCalc/codex-runs/x3-upstream`.

All 27 formula outputs and 28 native invocations agree, including every repeated
entry's ordered nested callbacks. All 17 owning-module frees use the returned
pointer on the calling thread, exactly once, before the next module entry.
Pre-close callback numbers/codes agree in order. Raw initialization differences
remain: FILTER element types, xlEventRegister result payload and Python
registration order. Excel unregisters by path; the host calls Remove then Close.
Those different teardown actions are reported separately.

`reload/unprotected` retains C0000005. `reload/same-host` and `reload/new-host`
retain exit-1 typed refusals for reopening the exact measured xlOil loader after
teardown. Refusals are failures, not compatibility passes. Safe xlOil reload is
dc-snz.21; Native AOT reinitialization remains dc-snz.19. Delivered events, RTD,
cancellation, workbook concurrency, general COM, ribbon/UI, xlwDotNet, Excel4 and
x86 are unverified. Locale evidence covers English interface/function syntax and
the measured Windows regional settings.

Replay each profile with the existing `verify.py`: give its Excel traced/control
directories, `--host`, `--host-control`, `--x3-cases` and this directory's
`source-publication.json`. Publication bindings use exact Git objects with only
LF/CRLF normalization. `sha256.json` covers the retained text files.
`corpus-comparison.json` proves all 21 complete verdict/evaluation payloads
unchanged; its counts are separate from these add-in cases. Full raw runs and
earlier failures remain at `C:/Work/DnaCalc/codex-runs/x3-runs`.
