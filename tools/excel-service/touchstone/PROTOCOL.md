# touchstone/1

Frames: a 4-byte big-endian length, a type byte, and a JSON body (compact, ASCII). A type byte of 0x80 or more means the
body is zlib-compressed and the real type is the byte minus 0x80. Frames stay under 64 KB. Local connections are not
compressed; the ssh stream between the broker and a relay is.

| Type | Byte | Direction | Body |
|---|---|---|---|
| hello | `H` | both | client: `{"v", "name", "token"?, "compress"?}`; service: `{"v", "server", "envs": {env_id: entry}}` |
| submit | `S` | client to service | `{"id", "seq", "last", "groups", "opts"?, "c"?}` |
| answer | `A` | service to client | `{"id", "seq", "env", "w", "b", "g": [...], "order"?, "c"?}` |
| done | `D` | service to client | `{"id", "calls", "envs", "seconds", "c"?}` |
| env | `E` | service to client | `{"env_id", "env"}` (before the first answer that names it) |
| error | `X` | service to client | `{"id", "seq", "code", "detail", "c"?}` (`refused`, `excel-crashed`, `excel-timeout`, `worker-down`, `auth`) |
| ping/pong | `P`/`Q` | both | echo |
| wait | `W` | client to service | `{"kind", "reason", "seconds", "lane"}` for the waits log |

## Requests
A request has an `id` (32 hex digits) and is sent as one or more `submit` frames with `seq` 0, 1, ...; the last has
`"last": true`. Frame 0 carries `opts`: `echo` (read back the inputs), `order` (`any` or `serial`), `priority`
(`interactive` or `bulk`), `calls` (the total, for the default priority), `primer` (serial mode: calls evaluated first).
`c` is a tag the relay adds and the service returns. A resent `(id, seq)` is answered from the service's cache for 15
minutes and never evaluated twice.

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
`f` supplies UTF-8 text content, which the worker writes to a temporary file under its session directory and passes as
a path. With echo, the range cells and file content are read back; a changed input is unread. These tokens use the
socket service and are currently unavailable in serial mode and the legacy file queue. Each call is evaluated separately.

## Answers
An answer frame holds groups `{"ty": "nnte...", "o" | "i", "v": [...]}`: one type letter and value per call: `n` bits,
`t` text, `b` boolean, `e` error name (`"#N/A"`, `"#SPILL!"`), `z` blank, `u` unread (`{"reason", "read_back"}`: Excel
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
