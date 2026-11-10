# X1 limits and observed differences (2026-11-01)

The passing comparison covers the original small fixture and its callback codes.
It does not establish general XLL or Excel-DNA compatibility.

| ID | Difference | Status and affected checks |
|---|---|---|
| DISC-001 | Excel 16.0.20527's direct MdCallBack12 xlAsyncReturn trace leaves result type zero; DNA Calc returns the Boolean TRUE documented by Microsoft. The callback code and published formula result agree. | Investigating in X2. The verifier records this difference and compares callback codes, not a claim of byte-identical callback results. |
| DISC-002 | All synchronous entries run on the owning main thread. `$` metadata is retained, but this host does not schedule concurrent worksheet calls. | Will fix with worker scheduling. Off-lane calls refuse; the fixture's single `$` result is tested, not parallel scheduling. |
| DISC-003 | A native async call waits for one completion with a bounded timeout. Tokens are never reused, and duplicate or late tokens are rejected. There is no cancellable workbook publication, batched completion, event registry or RTD integration yet. | Will fix in later async/COM work. MyAsync covers one successful background completion only. |
| DISC-004 | Excel4/XLOPER, legacy string/array and in-place return invocation are absent. The parser retains their type identities; invocation refuses unsupported signatures before calling them. The shim's Excel4/Excel4v return 32. | Will fix in X7. No legacy compatibility claim. |
| DISC-005 | Worksheet callbacks share CalcFml's existing worksheet-function route and one function-number table. Only the implemented host services, conversions and permission rules are available. xlStack answers the thread's headroom less Excel's measured 87,119-byte shortfall (x13 boundary batch, dc-6332.10); xlGetInst answers 32 as Excel does. Commands, full GET.WORKSPACE, nested arbitrary EVALUATE, UI, external services and COM objects are absent. | Will fix as their milestones implement them. Unsupported callbacks retain a typed dc-snz.2 refusal, including beneath IFERROR. |
| DISC-006 | The shim's Excel12/Excel12v forwarding and legacy refusal exports are compiled. The real fixture resolves MdCallBack12 in the executable; it measures XLCallVer and LPenHelper in the DLL. | Further forwarding-export execution checks remain open; no direct-call trace claim for those exports. |

Reference descriptions of callbacks do not replace the observed return codes.
Registration IDs, module paths, sheet handles, pointers and debugger times are
host-specific. The comparisons do not treat those identifiers or timings as values
that must equal Excel's. Raw captures retain them for inspection.
