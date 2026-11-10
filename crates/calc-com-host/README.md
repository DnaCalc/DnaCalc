# Local COM objects for the native host

This Windows x64 package answers the Application lookup measured in X0:
`WM_GETOBJECT` / `AccessibleObjectFromWindow(OBJID_NATIVEOM, IID_IDispatch)` on
EXCEL7, then Window.Application. Application implements Version, Hwnd and Run.
Run calls the registered XLL command on its owning STA with a command callback
frame; it does not activate another application.

The interface facts come from Excel 16.0.20527.20088's type library, read with
`LoadTypeLibEx(REGKIND_NONE)`. Application uses Excel's `_Application` IID
`000208D5-0000-0000-C000-000000000046`, its 474-slot dual vtable, and the observed
DISPIDs. The supported early-bound slots are Application 7, Parent 9, Run 42,
Version 281 (including LCID) and Hwnd 351. Window's IID is
`00020893-0000-0000-C000-000000000046`; it is a dispatch-only interface with
seven vtable slots. Its property descriptors must not be mistaken for a dual
interface's callable slots. Unimplemented members fail with HRESULTs.

The objects expose no type information yet. Managed callers tested here use
IDispatch name lookup; early-bound tests call the measured slots directly.
The AppEvents connection point retains subscriptions and supports Unadvise, and
delivers two application events to its sinks: SheetChange (DISPID 0x61c, worksheet
and range arguments in Excel's measured order, X4) and AfterCalculate (0xa34).
Other workbook and application events are not delivered.

The projection uses Windows' object-return mechanism and local objects only.
It has no class factory, ROT entry, activation hook, registry-writing code or
type-library registration. Excel's ProgIDs, CLSIDs and registered interfaces are
untouched. COM activation for a general desktop object model remains undecided.

X3 adds the observed Window.Parent workbook, Workbook.Application/Name/FullName/Path,
Application.Workbooks/Path/EnableEvents and Workbooks.Count/Item. The measured
Workbook dual vtable has 289 slots and Workbooks has 26. These objects describe
one local Book1 shell; they do not enumerate or project arbitrary application
documents. xlOil's original native and Python plugins use this route in the
[X3 captures](../calc-xll-reference/traces/2026-11-01-x3/).
