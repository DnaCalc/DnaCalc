# Rust COM projection for the native host

`calc-com-host` exposes the shared [object model](../../docs/architecture/OBJECT_MODEL.md)
to Windows x64 COM clients. `WM_GETOBJECT` / `OBJID_NATIVEOM` returns the Window object;
`Window.Application` reaches the application's supplied model. The projection implements
COM identity, Automation conversion and the COM-owned Office services in Rust.

`build.rs` generates complete vtables from `calc-object-model/descriptor/*.json`.
Every advertised interface has all its described slots. Early-bound slots and
`IDispatch::Invoke` decode to the same `Call`, including named arguments, optional
arguments and `VT_BYREF`. `GetTypeInfo` uses private in-memory type information built
from the descriptor with `CreateTypeLib2`; a Range's type name is `Range`.

The application creates `Projection::new(hwnd, host)` and supplies its semantics with
`bind_model(Rc<dyn ObjectModel>)` before activating add-ins. OneCalc uses
`profile::values_only::ValuesOnly` over the XLL application services. Its workbook
operations, including Caller and ThisCell, return Excel error 1004 with
"needs a workbook: open in DNA Calc" on both COM routes. The full workbook profile
is `WorkbookAutomation`; the projection holds no workbook state. The projection's
`EventSource` delivers descriptor events through per-source connection points,
snapshotting subscriptions before invoking handlers.

The Rust Office composition owns COMAddIns/COMAddIn connections, module leases,
bounded CommandBars and controls, AutomationSecurity and ribbon callbacks. The Rust
RTD client keeps the existing topic/declaration ledger and calls `IRtdServer` through
typed interfaces; its update sink uses the free-threaded marshaler. Ordinary object
calls stay on the owning STA (`RPC_E_WRONG_THREAD` elsewhere); retained objects
disconnect when the projection is dropped (`CO_E_OBJNOTCONNECTED`).

The native controls exercise real vtables and IDispatch:

```powershell
cargo test --locked -p calc-com-host -- --test-threads=1
```

Windows cross-compilation and linking on Linux do not execute those controls. First Windows
run 2026-11-14: these 29 tests pass, calcvba-udf 20/20 and X19 4/4 match Excel through DNA
Calc's page (codex-runs/om-acceptance-20261114/REPORT.md); the Excel-DNA and xlOil AppEvents
page comparison has not run yet. Unsupported members return the descriptor-named 1004 refusal and record
it through the supplied model; they do not read cached workbook answers.

The projection uses local objects and Windows' object-return mechanism. It neither
saves nor registers its type library, and it changes no Excel registrations or
security settings. The former C++ projection and RTD sources and their `cc` build
dependency have been removed.
