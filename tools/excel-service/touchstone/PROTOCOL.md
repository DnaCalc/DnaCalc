# touchstone/1

Frames: a 4-byte big-endian length, a type byte, and a JSON body (compact, ASCII). A type byte of 0x80 or more means the
body is zlib-compressed and the real type is the byte minus 0x80. Frames stay under 64 KB. Local connections are not
compressed; the ssh stream between the broker and a relay is.

| Type | Byte | Direction | Body |
|---|---|---|---|
| hello | `H` | both | client: `{"v", "name", "lane"?, "token"?, "compress"?}`; service: `{"v", "server", "sweep"?, "envs": {env_id: entry}}` |
| submit | `S` | client to service | `{"id", "seq", "last", "groups", "opts"?, "c"?, "origin"?}` |
| answer | `A` | service to client | `{"id", "seq", "env", "w", "b", "g": [...], "order"?, "c"?}` |
| done | `D` | service to client | `{"id", "calls", "envs", "seconds", "c"?}` |
| env | `E` | service to client | `{"env_id", "env"}` (before the first answer that names it) |
| error | `X` | service to client | `{"id", "seq"?, "code", "detail", "c"?}` (`refused`, `excel-crashed` for terminal request failures, `excel-crash-budget`, `excel-timeout`, `worker-down`, `auth`) |
| ping/pong | `P`/`Q` | both | echo. Broker-to-relay pings include `broker_flags`. |
| wait | `W` | client to service | `{"kind", "reason", "seconds", "lane"}` for the waits log |

## Requests
A request has an `id` (32 hex digits) and is sent as one or more `submit` frames with `seq` 0, 1, ...; the last has
`"last": true`. Frame 0 carries `opts`: `echo` (read back the inputs), `order` (`any` or `serial`), `priority`
(`interactive`, `bulk`, or `sweep`), `calls` (the total, for the default priority), `primer` (serial mode: calls evaluated first).
`addins` (a list of add-in file names, e.g. `["EUROTOOL.XLAM"]`) asks for an environment variant whose Excel has
exactly those add-in files open; only a worker the broker started with `--addin-worker` serves it (other requests never
reach it), and a request naming a set no worker has is refused. `c` is a tag the relay adds and the service returns.
The relay copies the socket client's `name` and optional `lane` into the submit's `origin` for diagnostics. The broker
provides the origin host from its relay connection or local machine. A resent `(id, seq)` is answered from the service's
cache for 15 minutes and does not trigger a new evaluation. When a batch for one request returns `excel-error`, the broker
can retry it once. A second `excel-error` for the same request returns terminal error code `excel-crashed` and refuses that
ID for one hour.

A sweep client requires `"sweep": true` in the service hello. A relay forwards the capability
from its broker. Sweep work is dispatched in small batches after foreground work, capped at
one worker by default while preserving the interactive reserve. A broker with a larger
`--sweep-workers` limit can expand while foreground workers are idle. The broker returns
`sweep-deferred` for sweep requests still queued after 12 seconds. Clients can retain
observations already received and retry the run from its recipe.

A **group** holds calls of one function and one argument shape, columnar:

```
{"f": "ERF.PRECISE", "k": "n", "n": 3, "v": [...n x width values...], "x": [r, c]?, "spill": true?, "o": 0 | "i": [...]}
```
`k` is one token per argument, comma separated: `n` number (16 hex digits of the binary64 bits), `s` text through a cell,
`S` text as a formula literal, `b` boolean through a cell, `B` boolean literal, `e` worksheet error (`"#N/A"`), `z` blank
(`null`), `aRxC` a numeric array of R rows and C columns, its R*C bit strings inline. `x` selects one output of a
regression function. `spill` asks for the whole array an array-returning function spills. `o` is the request position of
the first call (calls contiguous), `i` the list of positions.

`rRxC` supplies a real rectangular worksheet range, with R*C typed cell objects inline in row-major order. Cells are
finite numbers, text, booleans, blanks or worksheet errors; R and C are at most 32 and their product at most 128.
`mRxC` carries the same typed cells as an array expression rather than a reference.
`f` supplies UTF-8 text content, which the worker writes to a temporary file under its session directory and passes as
a path. With echo, the range cells and file content are read back; a changed input is unread. These tokens use the
socket service and are currently unavailable in serial mode and the legacy file queue. Each call is evaluated separately.

A **sheet case** (function `_CASE`, the one token `L`) lays out a small workbook and asks probe formulas in it:
`{"sheets": [{"name", "cells": {"A1": cell}, "hidden_rows": [n], "hidden_columns": [n]}], "names": {"Name": "=..."},
"probes": [{"at": "Sheet1!H10", "formula": "=ROW()"}]}`, a cell being `{"type": "number", "bits"}`, `{"type": "text",
"value"}` (in a text-formatted cell), `{"type": "boolean", "value"}`, `{"type": "error", "code"}` (entered as its error
formula) or `{"type": "formula", "formula"}`. The worker builds each distinct layout once per batch (one new workbook,
closed unsaved at the end; its sheets named and ordered as listed), constants as one Value2 rectangle per sheet,
checks they read back, then for each case enters its probes by Formula2, calculates once (CalculateFull for a layout's first case, then Calculate), reads every
probe and its spill range and clears them (contents and formats). Answer letter `c`: `{"probes": [{"ty", "v", "spill": matrix | null} |
{"refused": message}]}`; a layout whose constants changed is `u`. A probe with `"entry": "local"` is a typed entry, not
a formula: its text (any text of 1 to 255 characters) is entered by FormulaLocal, as the user's locale reads what is typed
into a cell (a number, date, time, percentage or currency takes the number format Excel gives it), and its answer adds
`"nf"`, the cell's NumberFormat after the entry (decoded as `number_format`). Send the cases of one layout together.

**Probe templates (dc-erfm3, the route to use).** A VBA case does not carry VBA source. It names a template from the
catalogue `tools/excel-service/vba-probes/catalogue.json` and gives typed parameters: `{"template", "params", "sheets",
"calls", "readback", "event_log"}` with no `modules`. Each template's module files (`vba-probes/*.bas`) are
reviewed once by the coordinator and pinned by SHA-256 in the catalogue. The worker (`VbaTemplates.cs`,
files embedded in the build) refuses a file that differs from its pin and builds the .xlsm from the pinned text and the
parameters only. Parameter types: `sheet` (a case sheet's name), `address` (one A1 cell), `text` (1..255 printable ASCII
characters, not starting with `=`, `+`, `-` or `@`; rendered as one quoted VBA string), `long` (an integer) and
`enum:a|b` (a listed word). Every `{{name}}` in a template is a declared parameter and every parameter must be given; a
call names one of the template's `entry_points`. `touchstone/vba-smoke.json` is the reviewed smoke case on template
`smoke` (it expands to the source in `vba-smoke-source.json`). To add a template: add the `.bas` files and a catalogue
entry with their hashes, and have the coordinator review them. The expanded modules still pass the grammar below. The
free-source `modules` shape below is no longer accepted on the wire (dc-45ndr): a case without `template` and `params` is
refused. It describes the expanded form the validators check. The approval digest binds the template name, its parameters and the
pinned module and layout hashes as well as the expansion. Approvals stay frozen.

A **VBA case** (function `_VBA`, token `V`) occupies one position in one request frame, encoded as `"o": 0` or `"i": [0]`.
The worker's decoder validates the group and case fields and rejects unknown fields. The broker invokes
`excel-worker --check-vba-policy` on its staged worker executable before queueing the case. The client uses the same
check when the local worker is available. `TOUCHSTONE_VBA_POLICY_WORKER` selects the local executable.
The case value contains `sheets` with typed number, text, boolean, error and blank cells, and the following fields.
The worker prints formulas and defined names from the template’s pinned layout and refuses request formula cells or
the `names` field. No workbook travels with the case: the
worker writes the source-only .xlsm itself from the checked `sheets` and `modules` (the sheet names, the `ThisWorkbook`
and `Sheet<N>` document modules, one project stream per module), so no uploaded compound file or compressed stream is
parsed. A case that carries a `workbook` field is refused.

| Field | Value |
| --- | --- |
| `modules` | Ordered `{"name", "kind", "source"}` objects. A `standard` module contains callable procedures. A `document` module names `ThisWorkbook` or a sheet code name `Sheet1`, `Sheet2`, … in layout order. |
| `calls` | Ordered `{"procedure": "Module.Procedure", "args": [...]}` objects. Entries name public procedures in standard modules. Arguments are typed scalar numbers, text, booleans, blanks, or worksheet errors. |
| `readback`, `event_log` | Ordered lists of `Sheet!A1` addresses. The probe's VBA writes event records to the requested cells. |

The broker sends one case per batch to the designated isolation slot. Echo, serial mode, primers, add-ins, and sweep
are unavailable on this route. The worker checks probe source against the grammar in `VbaGrammar.cs`
and `vba-probe-policy.json`: only the listed statement forms, expressions, built-in
functions and member chains of `ThisWorkbook`, its sheets and ranges, `Scripting.Dictionary`, `VBScript.RegExp` and `Err` are
accepted, and everything else is refused. A cell written through `.Value` or `.Value2` must be data: a number, a boolean, a
string literal that does not start with `=`, `+`, `-` or `@`, a case argument (text arguments may not start with those
characters either) or a concatenation of such; a value read from a cell, a dictionary or by a string function is never
written back. There is no `Application`, `Workbooks`, `Sheets.Add`, `Worksheets.Add`, unqualified `Range`, `Evaluate`,
`Run`, `Name ... As`, `Declare` or `CreateObject` outside the two classes. Standard module names match `[A-Z][A-Za-z0-9]{0,23}`
and are not reserved words; document modules are `ThisWorkbook` or `Sheet<N>` for a sheet in the case, and may define only
the listed `Worksheet_*` and `Workbook_*` event procedures. Sheet names match `[A-Za-z0-9_][A-Za-z0-9_ ]{0,30}`. The worker requires a local approval for the exact case before opening the workbook.
The approval digest binds the expanded source, layout, calls and readback addresses.
Reordering JSON object keys preserves the digest. Approvals remain frozen.

The worker opens the workbook read-only without updating links, applies the layout, calculates it, and enables events
for the `Application.Run` calls. It reads the requested cells and closes the workbook without saving.
Answer letter `v` carries `{"calls": [...], "sheet_state": [...], "event_log": [...], "case_sha256": "…"}`.
The digest identifies the worker's typed case. Each readback is
`{"at", "ty", "v"}`, and each call is `{"return": {"ty", "v"}}` or `{"err": {"number", "description"}}`.
A probe using a VBA error handler can return `Array("Touchstone.Vba.Err", CLng(Err.Number), Err.Description)` to
report those exact values. A COM invocation failure uses the service's worker error path. Workbook-local
`Workbooks.Open` calls are refused by the current source allowlist, since the request carries one workbook.

#### Ask from Linux

Save an authored case as `probe.json`. The client defaults
`names`, `readback` and `event_log` to empty.

```json
{
  "sheets": [{"name": "Data", "cells": {}}],
  "modules": [{
    "name": "Module1",
    "kind": "standard",
    "source": "Public Function Probe(ByVal x As Variant) As Variant\n    Probe = x * 2\nEnd Function\n"
  }],
  "calls": [{"procedure": "Module1.Probe", "args": [{"type": "number", "bits": "4035000000000000"}]}],
  "readback": ["Data!A1"]
}
```

Prepare the exact group for the coordinator to approve on the worker host. After approval,
capture the answer over the socket.

```sh
cargo xtask vba-case prepare probe.json > probe.group.json
cargo xtask vba-case capture probe.json --out ~/codex-runs/my-lane/vba-captures
```

`cargo xtask vba-case build probe.json > probe.xlsm` outputs a CalcDoc workbook for local inspection; Touchstone does not
use it. A Python caller
can use `Connection.vba_case(case, timeout=60)` from `touchstone.client`. It returns a
one-row reply with typed values and the Excel build, CPU, lane, and hashes. Refusals expose
the service code in `ServiceError.response["code"]`. The source checks refuse external
declarations, shell commands, file and network operations, dialogs, VBE access, and COM
activation outside the listed literal classes.

Each capture is stored under `<out>/<source_sha256>/<request-id>.json` with its typed case
and reply. The source hash covers the ordered module names, kinds, and source text. Compare
against a frozen CalcVBA prediction in the same form as `reply.rows[0]` to keep a bank row
only when the complete case differs.

```sh
cargo xtask vba-case witness capture.json prediction.json --out ~/codex-runs/my-lane/bank-rows
```

The WitnessSet keeps the procedure names, hex argument bits, and complete typed outcomes,
including event order and VBA Err details, with the capture's provenance.

## Answers
An answer frame holds groups `{"ty": "nnte...", "o" | "i", "v": [...]}`: one type letter and value per call: `n` bits,
`t` text, `b` boolean, `e` error name (`"#N/A"`, `"#SPILL!"`), `z` blank, `x` Excel crashed on this call run alone (`{"reason": "excel-crashed", "function", "k", "args", "detail", "worker"}`; never retried),
`u` unread (`{"reason", "read_back"}`: Excel
changed this input, or a spill passed the bound; the rest of the request stands), `m` a matrix
(`{"r", "c", "ty", "v"}`). With `echo`, `ity` and `iv` list the inputs Excel read back, `width` values per call. `env` names the
environment entry (`envs.jsonl`); `w` the worker, `b` the batch. Answers arrive in any order and in pieces: place them
by position. `done` follows when every piece is answered.

## Environment entry
`{"engine": "excel", "version", "build", "file_version", "machine", "os", "cpu_id", "cpu", "logical_cpus", "mt_calc",
"calculation", "iteration", "max_iterations", "max_change", "precision_as_displayed", "date1904",
"compatibility_version", "use_system_separators", "decimal_separator", "thousands_separator", "automation_security",
"com_addins_connected", "addins_installed", "scratch_sheets"}`; `env_id` is the first 12 hex digits of the SHA-256 of the
entry as the worker serialised it. A reply carries the id; clients cache the entries.
