# Excel answers for research agents (service v2, touchstone/1)

Python standard library only; the client works on Linux and Windows.
Several owned Excel processes answer typed function calls in batches; a lane on a Linux host reaches them through
one long-lived ssh session that the Windows machine opens. Measured on this machine (AMD Ryzen 7 4800H, Excel 16.0
build 20527, other load running): a single question from a Linux host p50 0.20-0.24 s, 10,000 calls in one request
0.32-0.33 s, 20 elementary functions at 200-270k calls/s over three workers, the ERF.PRECISE edge search 0.5 s
(v1: 73.7 s).

## Asking
From a DnaCalc checkout (or copy `client.py` and the `touchstone` folder beside it; `client.py` alone falls back
to the v1 file queue):
```python
import sys
sys.path.insert(0, "tools/excel-service")
from client import Client, number, text, boolean, blank, error, number_array
excel = Client()    # ~/.excel-service/excel.sock on a lane host, the broker's local endpoint on Windows,
                    # else the v1 file queue ~/excel-queue; Client(queue=...) always uses that queue
reply = excel.ask("ERF.PRECISE", [[number("3e47bc15e9263272")]])
print(reply["rows"][0]["bits"], reply["excel_build"], reply["env_id"])
future = excel.submit("ERF.PRECISE", rows)                # a concurrent.futures.Future
replies = excel.ask_many([("ERF.PRECISE", rows1), ("GAMMALN", rows2)])
for index, reply in excel.stream(requests): ...            # in completion order
run = excel.consecutive_run("ERF.PRECISE", "3e47bc15e9263000", 1000)   # up to 1,000,000 calls on the socket
edge = excel.bisect_edge("ERF.PRECISE", low_hex, high_hex, predicate)  # 64 points a round on the socket
```
`AsyncClient` offers `await ask`, `await ask_many` and `async for index, reply in as_completed(requests)`.
Options of `ask`/`submit`: `priority="interactive"|"bulk"` (up to 100 calls are interactive by default),
`order="any"|"serial"`, `echo_inputs=True` (the exact inputs Excel read back), `shape="array"`, `primer=[(function, row)]`.
Hex means 16 binary64 bit digits, not `float.hex()` notation; `number(1.0)` also works. Replies hold typed rows, the
Excel build and version, `env_id` and, with the v1 queue or `echo_inputs`, the exact input read-backs. Failures raise
`ServiceError` with its `.response`; a timeout raises `TimeoutError`. A reply to a call Excel could not answer
faithfully has `{"type": "unread", "reason": ...}` in that row (the v1 queue fails the whole reply instead).
`text(..., literal=True)` and `boolean(..., literal=True)` select direct formula literals; numbers always travel
through Value2 cells; `error("#DIV/0!")` and `blank()` use cells. Helpers vary one nonnegative finite argument;
`arguments=` supplies the others. LINEST, LOGEST and the four FORECAST.ETS functions take numeric arrays
(at most 128 cells, 32 rows or columns): `number_array([[2.0], [4.0]])`; select one LINEST output with
`result_index=(row, column)`. An unreadable lane report goes to `excel.report_wait(reason, seconds, kind=...)`.

Rust: `tools/touchstone-client` (native-only, serde_json and std sockets): `Client::connect()?.ask("ERF.PRECISE", rows)`,
`ask_many`, `submit` with a `Handle`. `PROTOCOL.md` in `touchstone/` describes the frames.

## Serial and array questions
- A function whose answer depends on the call before it (the registry: WEEKNUM, for return types outside 1-21)
  is always calculated call by call, in request order, in one single-threaded pass on the serial worker; the answer
  says `order: "serial"`. Any request can ask for it with `order="serial"`; `primer=` puts the engine in a known state
  first. (One `Range.Calculate` per call does not reproduce the state: it lives inside one calculation pass.)
- `shape="array"` for the array-returning functions (TEXTSPLIT, MINVERSE, MMULT, MUNIT, MODE.MULT, FREQUENCY, TREND,
  GROWTH, TRANSPOSE, SEQUENCE, SORT, UNIQUE): the answer row is `{"type": "array", "rows", "cols", "values"}` read from
  the whole spill range; `#SPILL!` and other error values are typed answers; a spill past 64 x 64 is `unread`.
- Leakage checks: one batch in a hundred is repeated in reversed order on another worker, and recent calls are
  repeated daily (and on an Excel build change) in a fresh Excel; differences go to `log/leakage.jsonl` as candidates
  for the registry. They never change an answer.

## What is admitted
All function names are admitted for tests except WEBSERVICE, STOCKHISTORY, IMAGE, TRANSLATE, DETECTLANGUAGE, PY and
COPILOT, which are deferred remote services. Arbitrary request formula text is not a protocol argument. Real worksheet
ranges and temporary file contents are available through `cell_range` and `file_argument` on the socket service.
Limits: 250,000 calls and 32 arguments per group; the v1 queue keeps 20,000 calls and 16 MiB per request. Workbook
captures and jobs that need VBA, add-ins, XLLs, events, iteration or pre-existing pivot data use
`tools/excel-session`; admitting a function name does not create that workbook context.

## Running it (Windows)
```powershell
py -3 tools/excel-service/touchstone/broker.py start --hosts dna-firehorse dna-koderbot `
    --local-queue "$HOME\excel-queue" --relay-v1-queue "~/excel-queue"
py -3 tools/excel-service/touchstone/broker.py status | stats --since 24h | waits --since 24h | heal | stop
```
Cutover: the v1 loop was stopped on 2026-11-09 at 01:05 and the broker started with the command above, so the existing
lane clients (the v1 file queue on both Linux hosts and on Windows) are answered by the broker unchanged. Roll back by
stopping the broker (`broker.py stop`) and starting the old loop under `tools/excel-session/on-private-desktop.ps1`, as
`run-private.ps1` did (a plain `service.py start` is not on the private desktop).
The broker and every Excel it starts run on the private desktop `DnaCalcExcelV2` under
`tools/excel-session/on-private-desktop.ps1`; its guard (`<state>/guard.json`, written at exit) must read
`windows_on_user_desktop: 0`. State folder: `C:\Work\DnaCalc\codex-runs\excel-service-v2` (`--state`). Three workers
by default (two with 4-thread multithreaded calculation, one with it off, which is the serial worker). A worker is
one C# process (`engines/excel`, `dotnet build -c Release`) that owns one Excel, bound through that process's own
window and checked by PID and start time. A missed batch deadline (5 s + 0.1 ms per call, or four times the batch's
estimated cost, or 20 s while a function has never been measured) kills only that Excel. The worker reports that kill as `excel-crashed` with `deadline: true`; the broker answers such a call (run alone, after the bisect)
as `excel-deadline` (answer letter `x`, with its arguments), records it in `deadline-list.jsonl` and counts it against
`--deadline-budget` (default 20 per hour, refusal `excel-deadline-budget`), never against the crash budget (dc-ztvv.26.130). `service.py stats|waits` read
the same logs.

Self-healing (excel-v2-2). A down worker (a crash, a lost worker process, a missed deadline or a failed start) is
started again after a cool-down (`--cooldown`, default 120 s), at most 3 restarts per worker per hour (`--restart-cap`);
a failed start leaves it down until the next cool-down window, so an Excel that cannot start is never hammered. At the
cap the worker stays down with the reason and the time of the next allowed attempt in `status --json`, and a
`worker-down` entry in `log/waits.jsonl`; `broker.py heal` clears the caps and retries at once. Functions that crashed
Excel twice are isolated (one-call batches); the list and the crash counts live in `<state>/isolated.json`, the learnt
per-function costs in `<state>/costs.json`, both read at start. `broker.py start` runs `dotnet build -c Release` on the
C# worker when `engines/excel/bin` is missing or older than its sources, and refuses to start (with the build output)
when that fails. On start the broker also stops worker Excels left behind by an earlier broker: only processes whose
command line names this state folder, each by PID and start ticks.

Crashes (dc-o2mw). A call that crashed Excel is an answer, never a retry. A crashed multi-call batch is not failed: each
of its items is marked suspect and goes back to the queue to run alone, and a lone item that crashes again is halved,
until the crashing call stands by itself. A call that crashed Excel as a one-call batch comes back to its caller as the
typed result **excel-crashed**: answer letter `x` (`{"reason": "excel-crashed", "function", "k", "args": [raw group
values], "detail", "worker"}`) in `raw()`, and in the v1-shaped reply a row `{"type": "crashed", "reason":
"excel-crashed", "function", "arguments": [typed arguments], "worker", "detail"}`; `client.is_crashed(row)` tests for it
and the other calls of the request are answered as usual. (Through the v1 file queue and strict replies a crashed row
fails the reply, as `unread` does.) Every such call is appended to `<state>/crash-list.jsonl` (utc, function, token
shape `k`, raw `args`, worker, batch, request ID and origin) for the handbook. Suspect calls and isolated functions run
alone on the isolation worker (the last non-serial worker, `w2` by default). During its cooldown, restart or outage, the
broker delegates this work to one available worker without add-ins. Isolated crashes handled by bisection or a typed
crash answer leave that worker's restart budget available. A function with `CRASH_BUDGET` (6, `--crash-budget`) crashed calls in an hour has
its remaining suspect calls refused, not run: `ServiceError` `excel-crash-budget: ... not run` (the call was not
evaluated; resubmit when the crash is understood). The restart cap never takes the last worker that is up: that worker
restarts again after each cool-down.

If Excel returns `excel-error` on a batch belonging to one request, the broker retries once. A second such failure
ends the request with `excel-crashed`. Its ID is refused for one hour across clients and broker restarts, and the
received payload is recorded in `crash-list.jsonl`. Keep that file in the broker state directory when investigating.

### Broker incident runbook

1. Run `broker.py status` on Windows. It reports `OK`, `DEGRADED` or `DOWN` with exit codes 0, 1 and 2. Status older
   than 15 seconds reads as down. Use `status --json` for `oldest_queued`, `queue_by_function`, and each worker's
   `queue`, `down_reason`, and `retry_at_utc`. Queue entries show call counts and the oldest request's ID, function,
   origin, age and waiting reason. Shared work is attributed to a likely worker for display and can dispatch elsewhere.
2. A queued isolated call older than 10 seconds raises `isolated queue stalled` after 10 seconds without an isolated
   dispatch. The Windows status
   exposes `broker_flags`, and `health.txt` repeats the health line every 30 seconds. Relays publish the flags every
   five seconds in `<socket>.health.json` next to the lane socket. Check `received_at` before using a relay report.
   Find the request by ID in the batch log and look in `crash-list.jsonl` for the payload of a quarantined request.
3. After correcting a failed worker's underlying problem, run `broker.py heal` to clear the caps for down workers
   and retry their starts at the next tick. Queued work and quarantined request IDs stay in place.
4. For deployment or a wedged broker, coordinate with callers. Wait for active and queued work to drain when the
   broker can still answer. Run `broker.py stop` and wait for it to confirm, then start through the existing scheduled
   task with its usual configuration. A stop drops work still in memory, and clients may need to resubmit it.
   Confirm worker and relay recovery with `status` after startup.

Capacity and fairness: interactive requests (up to 100 calls, or `priority="interactive"`) are taken first and bulk never
fills the last worker, so a one-call question always finds a free Excel; bulk is shared per client (per relay client
behind a relay) by estimated Excel time, not call count; a batch is cut to about 0.8 s of estimated work (a batch also
pays ~150 ms of fixed fingerprint and COM time under load). A function never measured runs alone as a 50-call probe
that sets its cost.

Logs: `log/YYYY-MM-DD.jsonl.gz` (one record per batch with worker, `env_id`, calls, cells, millisecond stages, and
request IDs with origin labels). New Python socket clients send the client name and lane from `DNACALC_LANE` or the
launcher log directory. Relays supply the host. Older socket clients and v1 file callers have partial origin labels.
`envs.jsonl` (one entry per Excel environment: version, build, file version, bitness, OS, CPU, multithreaded
calculation, scratch settings; `env_id` is its hash), `log/waits.jsonl` (requests over 1 s + 10 us per call, queueing
over 250 ms, refusals, worker down, lane reports; classes transport, queue, excel-calc, excel-start, refused-kind,
job-route, client-timeout), `log/leakage.jsonl`. No per-request folders.

## v1 compatibility
The file queue still works: a Linux relay serves `~/excel-queue` (a v1 `Client` there asks in about 0.25 s) and the broker
serves the Windows `~/excel-queue`; both answer in the v1 reply shape (`inputs`, `excel_build`, `machine`,
`calculation_settings`). `service.py start|stop|run` is the old loop (one Excel through the session host); it and the
broker must not serve the same queue at once. Notes the v1 service made still hold: a lone UTF-16 surrogate in text
is now passed through as written (v1 replaced it with U+FFFD in the cell, and its `inputs` showed that).

Tests: `python -m unittest discover -s tools/excel-service -v` (v1 protocol and shim, and the touchstone broker, batcher,
client and relay-free tests over a stand-in worker); `cargo test -p touchstone-client`.
Proofs, with their measured results: `touchstone/worker_proof.py`, `proof_wp2.py`, `load_test.py`, `proof_wp3.py`,
`proof_wp5.py`, `bench.py` (`--host X --calls N --function F` measures from that host).
`prove.py --reference <ERF.PRECISE.json> --output proof.json` runs the live edge proof.
