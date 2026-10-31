# VBA functions in DNA OneCalc

`dc-snz.3`, 2026-10-31. The owner deferred the OxVba rename and requested the UDF
integration first. `calc-vba` pins OxVba's embedding API at
`74beca5d5bea0370103be3584f18825640eeb5c3`; both desktop and browser use the interpreter,
with the JIT disabled. This does not rename the engine or introduce a calculation engine
into OneCalc.

In OneCalc, load the example in **VBA functions**, enter `40` in A1, and enter
`=Add2(A1,2)` in the formula field. The result is 42. Edit the module and load it again
to replace its functions. A compile failure leaves the previous module active; the
status says so. Unload removes its registrations. Nothing is persisted to a workbook.

`calc_func::UserFunction` and `UserFunctions` are the host-neutral eager value-call
contract and caller-owned registrations. `calc_fml::compile_with_functions` resolves
names after built-ins, local bindings and known defined names. A catalogue that cannot
say whether a defined name exists still refuses that ambiguity. Evaluation receives the
current registry through `EvaluationServices::with_functions`. Plans hold names, never
VBA engine objects; replacement and removal cannot run old code. Hosts must recompile when
adding a previously unknown name, because an unknown call compiles to #NAME?. OneCalc compiles
on each calculation, sharing only plans with the same resulting instructions. User calls conservatively
mark the plan volatile. Workbook qualification and richer effects metadata remain future work.

Only `calc-vba` depends on OxVba. It compiles public standard-module Functions and registers
bare and module-qualified names with ASCII case matching. Subs and Private functions
are not registered, and Main never runs. Calls start with fresh globals/Static/Err and
private ByRef arguments. The default host provides no external services.

The formula evaluator reads a single-cell reference as its value and a rectangular range
as a two-dimensional value array. Numbers, booleans, blanks, valid UTF-16 text, known
worksheet errors and rectangular arrays cross the adapter. Returned worksheet errors keep
their code. An unhandled VBA error, exhausted budget, wrong arity or conversion failure
becomes `#VALUE!` with a diagnostic; IFERROR can catch the worksheet error. Missing
registrations are `#NAME?`. VBA execution stops at the per-call basic-block budget
(100,000 in the UI), including nested VBA calls; On Error cannot catch exhaustion.
The budget does not interrupt compilation, conversions or an executing external host call.

Still open: real Excel Range/Caller/WorksheetFunction objects, persistent project state,
omitted Optional arguments, ParamArray packing, locale-sensitive identifiers, workbook
loading/macros/events and the desktop application shell (`dc-snz.1`). A Range is not
represented as an object by this value-only integration. These limitations must remain
explicit under `dc-snz.5` (and OxVba `bd-59co.2.10.6`); array conversion is not a claim of Range-object parity.

Validation uses `apps/dna-onecalc/tests/vba.rs` for the native/browser controller route
and `cargo xtask accept --id dc-snz.3 --platform windows|wasm --require-all` for Add2;
the wasm acceptance case also loads/reloads/unloads through the mounted UI and runs an
infinite loop under the budget. `cargo xtask app smoke onecalc` tests the bundled page.

Run evidence: the native OneCalc example prints `number 42`; the bundled page passes
31 smoke cases in each of Chrome and Firefox, including the content policy and narrow
layout. The corpus remains 19,284/20,030 formula cells matched, 659 failed, with 73
external-link values and 14 circular-rule results counted separately (96.27% matched
over all cells). Native and mounted-browser acceptance uses the same controller.
