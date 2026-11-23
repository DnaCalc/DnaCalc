# CalcFunc bindings for Python, .NET and JavaScript

Expose Excel worksheet functions over values supplied by the caller. Generate all three
APIs from CalcCatalogue, attach the handbook documentation by stable function ID, and run
each call through the same Rust argument preparation, lifting and CalcFunc kernel route.

## Boundary and ownership

| Binding | Entry point | First targets |
|---|---|---|
| Python (dc-dfl.2.2) | PyO3 with the stable Python ABI (`abi3`), generated Python signatures and `.pyi` stubs | Windows x64 and Linux x64 wheels |
| .NET (dc-dfl.2.3) | Generated C# over P/Invoke and a versioned Rust C ABI crate, `calc-func-ffi` | NuGet with `win-x64` and `linux-x64` native runtimes |
| JavaScript (dc-dfl.2.4) | Generated TypeScript over wasm-bindgen and the soft-x87 build | npm for Node and browsers, synchronous calls after asynchronous initialization |

Use PyO3 to meet the Python wheel's stable-ABI requirement and to own Python conversion and
thread-state handling in one adapter. A shared C ABI with cffi would reduce the number of native
entry points but would still need a Python packaging and marshalling layer. Share the portable
Rust call adapter below PyO3, P/Invoke and wasm-bindgen. Use one generic dispatch by stable
`FunctionId` plus ABI version and disposal functions. Generate public worksheet function names
and signatures above that dispatch.

The C ABI uses fixed-width tags, length-delimited UTF-16 strings, and dimensioned row-major
array buffers. Input buffers are caller-owned and borrowed for the call. Validate tags, shape,
checked dimensions and lengths before decoding. Rust owns returned variable-length storage
under an opaque result handle. Its borrowed views stay valid until the adapter copies them and
releases the handle with the exporting library's disposal function. Fixed-size scalar results
can be written into caller-owned storage. Use stack-backed scalar descriptors and span-based
bulk inputs in .NET. NumPy columns and JavaScript `Float64Array` columns cross in one batch.
PyO3 converts to owned Rust inputs while attached to Python, detaches from the interpreter for
evaluation, then converts results after reattaching. JavaScript marshals into wasm and copies
out result buffers before releasing them.

Keep scratch and service state scoped to a call. Sessions hold immutable profile defaults and
can be shared across threads. Supply an independent random stream per evaluation, or have the
caller explicitly synchronize shared stream state. Native entry points translate unwinding Rust
panics into internal binding failures. A wasm trap is a binding failure as well. Use distinct
channels for internal failures, invalid binding inputs, typed CalcFunc refusals and successful
worksheet values. In particular, a fault from
[`calc_func::invoke`](../../crates/calc-func/src/call.rs) retains its identity and owning work
package where supplied.

## Catalogue and generation

[`WorksheetDeclaration`](../../crates/calc-catalogue/src/declarations.rs) already records
stable IDs, spellings and localized names, entry kind and availability, `CallableArityShape`,
optional positions through arity, repeating `ArgumentGroups`, parameter names, argument demand,
`ResultForm`, `EntryTraits` and optional `ContextFacility` classifications. `ParameterClass`
already gives the call interface's reference policy. `Value` dereferences before the kernel,
`ValueOrReference` preserves references, and `Reference` requires one. `EntryClass` describes
operand admission at formula entry. `LegacyPlace` describes legacy intersection and array
placement. `ResultForm` describes whether a call may yield a reference or callable. A concrete
scalar or array shape can still depend on arguments and lifting.

Complete the declarations where generation needs more information. Add public operand
domains and coercible forms (number, text, logical, array or reference as applicable) beyond
`EntryClass`'s broad admission classes. Record measured omitted-argument defaults with their
conditions, complete context-facility classifications, and describe any statically known
result shape where it informs types. An unclassified context is unknown. Keep the omitted
marker intact through invocation so the kernel applies its own default. Generate concise
typed overloads plus a value-oriented form for Excel coercion and error handling. Preserve
aliases, optional holes, repeating groups and alternate signatures according to the declaration.
A trailing absent argument and a `Missing` marker inside the argument list need distinct encoding.

Use `calc_catalogue::parameter_lifting` for each zero-based call position, including repeated
positions. Supply the formula mode, the evaluated argument's origin, and the enclosing call
position for a legacy formula. The [generated table](../reference/parameter-lifting.tsv) gives
the same policy for the bindings and handbook, with inclusive one-based position spans.
The counts cover each position, mode and origin. Ordinary arrays, contiguous multi-cell ranges
and arrays of references have separate columns. For example, COUNTBLANK takes a range whole
and lifts a computed array. The table describes pre-publication call behavior. Excel build
availability, formula entry, 3-D references, boxed arrays and callable values have separate rules.

`elementwise-when-lifted` means a lazy IF or CHOOSE branch participates only when the selector
lifts. In a legacy aggregate placement, a scalar selector can return a whole range from the
branch. A lifted selector gives a typed fault for that range (`refused-when-lifted`). IFERROR
and IFNA can also lift on a demanded fallback. IFS and SWITCH use the `whole-result` column to decide
when scalar selection keeps the chosen result whole. Use CalcFml's argument preparation and
dispatch for these value-dependent decisions.

`unmeasured` records typed refusals for unclassified legacy placements and held-reference
arrays (dc-b1l), and for deferred functions (dc-fetch-f0tp and dc-nk93). `special-form` marks
formula constructs that have their own routes, such as LET and LAMBDA. `unavailable` records
worksheet admission refusals. Check `availability` as well as lifting when generating an API.

## Values and reference origin

| Excel value | Public representation and rules |
|---|---|
| Number, text, logical | Native double/float, string and Boolean for ordinary values. Keep Boolean distinct before numeric conversion. |
| Worksheet error | Python and .NET `ExcelError` enums and a JavaScript `ExcelError` value object, with the current CalcValue error codes including `#N/A`, `#VALUE!`, `#SPILL!` and `#CALC!`. Return errors as values by default. An opt-in raising policy applies after computation, reporting the first error in row-major order with its coordinate for an array. |
| Empty and omitted | Distinct `Empty` and `Missing` sentinels (including .NET `ExcelEmpty` and `ExcelMissing`). An empty cell participates in range reads. In `PMT(r,n,pv,,1)` the fourth argument is `Missing`; in `PMT(r,n,pv,0,1)` it is a numeric zero. The binding preserves both inputs even where the kernel's default gives equal results. |
| Range | Plain containers are ranges: Python lists, NumPy arrays and DataFrame columns, .NET arrays or spans, and JavaScript arrays or typed arrays. `Range` supplies explicit dimensions and optional cell facts. `cell(x)` creates a 1×1 range, equivalent to `Range([[x]])`. |
| Excel array | Rectangular, mixed-type, row-major values with an `ExcelArray` tag and explicit shape, including 1×1 results. Array results carry this tag and keep array semantics when passed back. Provide `ExcelArray` construction for a literal Excel array and explicit conversion to each language's plain containers, such as nested lists and `object[,]`. |

Scalars are literal arguments. Each argument can independently be a literal scalar, a range or
an Excel array. For example, `SUM([["3"], [true], [2]], "3")` takes 2 from the range and 3
from the literal. `SUM("3", true)` coerces both literal arguments and yields 4. The same origin
distinction is available to COUNT, AVERAGE, MATCH, the IFS family and SUMPRODUCT through their
normal argument preparation. Kernels own the coercion.

A flat sequence is a column. Use nested rows or an explicit shape for a row vector. Reject
ragged containers, zero-dimensional extents and nested element containers as invalid binding
inputs. Ranges use a call-scoped in-memory cell reader and synthetic reference identities.
Optional range metadata supplies an origin for legacy intersection and cell facts such as
filtered rows, hidden rows and nested subtotal markers for SUBTOTAL and AGGREGATE. Declare
supplied cells visible and unmarked by default. Preserve the shape of referenced results such
as INDEX over an in-memory range by reading their values before returning a tagged array.
Scalar value results use the ordinary scalar representation.

Use the value distinctions in [`CoreValue`](../../crates/calc-value/src/value.rs). Preserve
binary64 input and output bits across the boundary, including negative zero and subnormals.
Carry numeric wire values as 64-bit words (or two 32-bit words in wasm glue). Offer a tagged
bit-preserving representation for exceptional values whose payload a language number may
canonicalize, particularly NaNs in JavaScript. Preserve any CalcValue calculation provenance
that affects numeric round trips, including the internally qualified rounding-overflow value.
A raw caller-supplied NaN carries only its bits. Excel's kernel and publication rules determine
whether any such input is accepted, flushed or produces a worksheet error. Test bit transport
separately from those worksheet rules.

## Calls with array and workbook semantics

- **Lifting and intersection.** Default to dynamic-array formula semantics. Prepare references
  per slot, then run the shared Rust lifting path once per public call. The CalcFml
  [shape and element rules](../../crates/calc-fml/src/eval/lift.rs) use the largest row and
  column extents of the lifted inputs. A 1×N row with an M×1 column broadcasts to M×N.
  Length-one axes repeat. Other short axes supply `#N/A` at uncovered coordinates, with error
  precedence owned by the evaluator and kernel. Retain whole-argument behavior and
  per-reference lifting when declared. A legacy scalar intersection uses the supplied
  calculation anchor and range origin where required. If that context is missing, return a
  typed refusal. Return shape and element errors as data without attempting worksheet spill
  placement. Nested array construction produces `#CALC!` where Excel rejects it. Preserve
  CalcFml's measured first-cell projection where a lifted kernel returns an array, as in the
  `TEXTSPLIT` and `MUNIT` witnesses documented in `lifted_element`.
- **Locale and date system.** Use the existing
  [`FormatProfile` and `CalcEnvironment`](../../crates/calc-func/src/locale.rs), defaulting
  to en-US and the 1900 date system. Pass the selected locale to VALUE, DATEVALUE, TEXT and
  number formatting. Date arithmetic uses Excel serials. Serial conversion helpers for
  Python datetime, .NET DateTime and JavaScript Date take the selected 1900 or 1904 system
  explicitly or from the session. Preserve serial 60 in the 1900 system as an Excel serial
  or tagged fictitious date. Civil date conversions use explicit date/time components so a
  host time zone cannot shift the calculation.
- **Text.** Carry UTF-16 code units losslessly, including lone surrogates where present.
  Apply the 32,767 UTF-16-unit worksheet text limit using the existing Excel text path.
  Supply workbook compatibility version so LEN, MID and FIND get the measured astral
  character behavior for that version. Comparisons, MATCH, SORT and lookups use the existing
  Excel collation path. Surface unsupported profiles or collation cases as typed refusals
  where the shared implementation does. Spreadsheet number-to-text uses the
  [Excel 15-significant-digit publication path](../../crates/calc-func/src/number_text.rs)
  and existing number formats. Binary64 results retain their bits.
- **Volatility.** Inject a clock snapshot through `CalcEnvironment::now` for NOW, TODAY and
  other clock consumers. Inject `RandomSource` through `EvalServices` for RAND,
  RANDBETWEEN and RANDARRAY. Accept frozen inputs or a caller-owned seeded stream for replay.
  Materialize the services before evaluation so the call never re-enters a foreign runtime
  for a clock or random draw. Report a typed context failure when a needed service is absent.
- **Callables and workbook services.** The first release excludes callable-taking functions
  (MAP, REDUCE, SCAN, BYROW, BYCOL, MAKEARRAY and other LAMBDA consumers) and foreign
  callbacks. Exclude workbook- and host-bound functions such as ROW, COLUMN, CELL, INDIRECT,
  OFFSET, ISFORMULA, FORMULATEXT, INFO, RTD, WEBSERVICE, CUBE functions, GETPIVOTDATA and
    IMAGE. Publish the specific availability reason from the completed catalogue. The separate
    CalcFml formula API (dc-dfl.2.7) owns the small workbook model, formula parsing, addresses,
    workbook facts and Excel LAMBDA. Services with no provider remain unavailable there.

## Profiles and fidelity

Use DNA Calc's [`NumericMode` and `NumericTargetId`](../../crates/calc-value/src/numeric.rs).
An exact target identifies the Excel build, channel, compatibility setting and recorded numeric
environment, including CPU where it matters. Carry the effective profile per call, with optional
session defaults and explicit call overrides. Include formula mode, workbook compatibility
version, locale and date system in evaluation provenance alongside the numeric target and backend.

Ship soft-x87 by default on all targets, including the native packages. Keep an explicitly
selected native-x87 option for supported and measured local targets. Native transcendental bits
can differ by CPU. The current soft-x87 feature is a build-time choice.
[The x87 evidence](x87-in-calc-func.md) also records occasions where current software and
hardware answers differ. A portable backend gives one intended cross-platform policy. An
`Exact(target)` promise requires evidence for that target and a backend able to serve it.
The current CalcFunc numeric site can report exact mode unavailable. Return that refusal for
unsupported requests and publish the measured portable result status by target. Any later
improved numeric mode needs an explicit profile selection and evidence of its own.

## Verification and release

Run the full witness bank and pinned sets through the public binding entry points on each
supported runtime. Use the same witness records and verdict path as `cargo xtask witness judge`.
Compare binding outputs to CalcFunc under the same profile and inputs bit for bit, including
array shapes and all typed elements. Compare to the recorded Excel answer independently.
Retain every bank row in the totals, with unsupported, missing, mismatched and refused rows
counted explicitly. Exercise the selected clock and random source, scalar/range/array origin,
legacy intersection, uneven broadcasts, omitted arguments, UTF-16 text and number-bit
transport. Run Windows and Linux package tests, a full Node run and a browser smoke test.
Apply the same comparisons when adding future runtimes.

Test generation against the canonical catalogue export, covering public names, parameter
order, optional holes, repeating groups, signatures, stubs and generated handbook text.
Generate package status tables and the handbook's “Use it in code” records from
[`witness-status.tsv`](../../crates/calc-func/witness-status.tsv) and its corresponding
profile-specific witness runs. Include the backend and numeric target, CalcFunc revision,
counts, differences and unavailable reasons. Where existing status rows mix builds or omit
CPU, retain their provenance and publish the default profile as unmeasured until the matching
run exists. Package docstrings, XML documentation and JSDoc use the corresponding handbook
records, with the same signatures emitted in handbook API JSON (dc-dfl.2.5).

After Windows and Linux x64, add macOS native packages. Use the same soft-x87 Rust build policy
for npm/wasm, Pyodide and Blazor, with each runtime's own marshalling adapter. The Python, .NET
and JavaScript package beads own their respective implementation and verification. Publishing
requires the owner's approval and the licence work in dc-96l.
