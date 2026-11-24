# calc-js-host

This crate attaches Office.js custom functions to CalcFml's `CallableTable`.
Pass the namespace from the manifest, the contents of `functions.json`, and a
classic JavaScript script containing the add-in's `CustomFunctions.associate`
calls. Use a source ID ending in `.mjs` to load an ECMAScript module.
Use the metadata's ID for the association and namespace plus name in formulas.
The owner retains the `JsAddin` while it publishes its table. Dropping the
owner revokes execution through previously published tables.

The native backend is `engine-quickjs`, enabled by default. It uses the MIT
licensed rquickjs and bundled QuickJS sources from Cargo dependencies.
`engine-boa`, `engine-v8`, and `engine-browser` are reserved for later
backends. On wasm the adapter and metadata compile without a native runtime.
Each backend implements `JsEngine` and runs the same conformance cases.

The synchronous boundary passes scalar and rectangular matrix worksheet
values, packs repeating arguments, and maps omitted optional arguments to
JavaScript null. Invocation metadata comes from the existing CalcFml caller
and argument references. Volatile functions report the evaluator's normal
volatility effect. JavaScript receives values and invocation metadata through
a bounded runtime with no I/O, module resolver, or workbook bridge. The
native loader in `calc-addin-host` rejects Off before parsing or starting code.
Guarded and Unguarded use the same embedded runtime capabilities.

The QuickJS heap limit applies to the full runtime and the wall deadline
uses QuickJS's interpreter interrupt plus checks during value conversion.
The host also bounds decoded results. Limits apply at module initialization
and invocation. Exhausting a limit produces an execution diagnostic.
A completed worksheet value survives deadline cleanup.

Promise completion and cancellation belong in the adapter's
`UserFunction::begin_call` path and an extension to `JsEngine` for pending
jobs (dc-26wtr.6). Streaming can extend the same invocation boundary for
`setResult` and `onCanceled` (dc-26wtr.7). The synchronous adapter emits
typed refusals for these calls. The shared-runtime webview and manifest
discovery are part of the Office.js add-in host (dc-17spk.5). Excel-backed
cross-UDF parity, including coercions and edge cases, belongs to dc-17spk.22.

The metadata and call conventions follow Microsoft's
[functions.json documentation](https://learn.microsoft.com/en-us/office/dev/add-ins/excel/custom-functions-json)
and [parameter options](https://learn.microsoft.com/en-us/office/dev/add-ins/excel/custom-functions-parameter-options).
The error API follows the documented
[custom function error types](https://learn.microsoft.com/en-us/office/dev/add-ins/excel/custom-functions-errors).
