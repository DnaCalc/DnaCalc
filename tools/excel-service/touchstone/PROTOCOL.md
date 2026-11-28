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

**Probe templates.** A VBA case names a template in
`tools/excel-service/vba-probes/catalogue.json` and gives its typed parameters. The wire fields are
`template`, `params`, `sheets`, `calls`, `readback`, and `event_log`. Request sheets carry typed
number, text, boolean, error, or blank cells. Formula cells, defined names, modules, and workbook
bytes are refused. Formulas and defined names come from the template’s SHA-256-pinned layout.
The worker builds the workbook from its templates and writes the typed values to cells.

A **VBA case** (function `_VBA`, token `V`) occupies one position in one request frame, encoded as
`"o": 0` or `"i": [0]`. The worker's decoder validates the group and case fields. The broker calls
`excel-worker --check-vba-policy` using the staged worker before queueing the case on the isolation slot.
The client invokes the same decoder when the worker is available locally. Set
`TOUCHSTONE_VBA_POLICY_WORKER` to select the local executable. Echo, serial mode, primers, add-ins,
and sweep are unavailable for VBA cases. A case sheet is not named `History` (Excel reserves it).

| Field | Value |
| --- | --- |
| `params` | The exact parameters declared by the template. Types are a case sheet, an A1 cell, printable ASCII text, a VBA Long, and closed lists of strings. |
| `calls` | Ordered `{"procedure": "Module.Procedure", "args": [...]}` objects. Each procedure and its argument types are declared by the template. The smoke probe accepts one binary64 number and its error probe takes no arguments. |
| `readback`, `event_log` | Ordered lists of `Sheet!A1` addresses. The template can write event records to cells in the case workbook. |

Run `cargo xtask vba-case admit` with a built worker to validate the embedded catalogue. Give it a
catalogue directory to preview a change before editing the embedded catalogue. The worker build runs the
same admission check. It verifies the raw source and layout pins, checks the template's VBA grammar with
typed literal tokens, and checks where each placeholder goes in the layout. Module placeholders provide
VBA string or numeric literals. Layout placeholders provide quoted sheet references or formula literals.
The admission check runs on template text. At request time the worker checks the pins, decodes values,
renders the template, and reads the rendered modules and formulas with the same grammar before it builds
the package. A worker whose embedded catalogue fails admission refuses every VBA case.

What a template may say (dc-heiq6; `VbaGrammar.cs`, `VbaLayoutFormula.cs`, `vba-probe-policy.json`):

- A module begins with `Option Explicit`. A comment does not end in `_` (Excel continues it over the
  next line). A local or parameter name is not also a module variable or a procedure. A line stays
  within 1023 characters with every parameter at its longest, and a statement within 24 continuations.
- A procedure is not named like a worksheet function, alone or as `Module.Procedure` (`Register.ID`),
  nor `Auto_...`. `Err.Raise` takes a number, a source and a description.
- A cell takes a number, or text whose first character that is not a blank (space, tab, line break) is
  not `= + - @`. Number text may follow only a lead with such a visible first character, so `" " & n`
  and `vbTab & n` are refused and `"n=" & n` is accepted. A `text` parameter obeys the same rule.
- A parameter of a standard-module procedure counts as a number only when its type is numeric: a
  formula can call the procedure with any cell text (`Private` does not prevent it). Write `CDbl(x)`
  to a cell.
- `Range` takes a literal A1 cell or rectangle (at most 1,048,576 cells) or an `address` parameter.
  A computed cell is `Cells(row, column)`. Indexes of `Cells`, `Rows`, `Columns`, `Item` and `Offset`
  are numbers, numeric variables, arithmetic on them, or one call such as `CLng(...)`. A worksheet's
  `Cells`, `Rows` and `Columns` need an index. `Resize` takes two whole-number literals up to 1024.
- In a worksheet module `Range`, `Cells`, `Rows` and `Columns` are the module's own sheet. Elsewhere
  they are refused.
- A layout formula holds numbers, strings, `TRUE`/`FALSE`, error values, A1 references (alone or after
  a case sheet name), the layout's own names, operators, and calls of the functions listed in
  `layout_functions` or of a public function of the template.

The grammar does not decide whether a probe ends. A loop, a recursion, an event that writes the sheet
it listens to, or a write over a large used range runs until the request's deadline, when the worker
ends its Excel. The smoke template guards its change event with a flag for that reason.

The approval pin covers the template’s module and layout pins, parameters, entry-point signatures, and
review record. That record identifies the reviewer, the claim covering every declared input, and the
Excel build family. The worker requires a recorded review and a matching local template approval before
opening the workbook. The case digest identifies the exact typed case and its template, independent of
object key order. The smoke template is pending review and approvals remain frozen.

The worker opens the workbook read-only without updating links, applies the layout, calculates it, and enables events
for the `Application.Run` calls. It reads the requested cells and closes the workbook without saving.
Answer letter `v` carries `{"calls": [...], "sheet_state": [...], "event_log": [...], "case_sha256": "…"}`.
The digest identifies the worker's typed case. Each readback is
`{"at", "ty", "v"}`, and each call is `{"return": {"ty", "v"}}` or `{"err": {"number", "description"}}`.
A probe using a VBA error handler can return `Array("Touchstone.Vba.Err", CLng(Err.Number), Err.Description)` to
report those exact values. A COM invocation failure uses the service's worker error path. Workbook-local
`Workbooks.Open` calls are refused by the current source allowlist, since the request carries one workbook.

#### Ask from Linux

Use `touchstone/vba-smoke.json` as the request example. The client defaults
`readback` and `event_log` to empty. With a built worker, `prepare` validates the case locally and
prints its group. A capture requires the coordinator to have approved the template and enabled use
after independent review.

```bash
cargo xtask vba-case prepare tools/excel-service/touchstone/vba-smoke.json
cargo xtask vba-case capture tools/excel-service/touchstone/vba-smoke.json --out captures
```

The capture retains the case and its one-row reply with typed values and provenance. The source hash
identifies the template and parameters, and the worker's case hash covers its typed case and pins.
Compare against a frozen CalcVBA prediction in the same form as `reply.rows[0]` to keep a bank row
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
