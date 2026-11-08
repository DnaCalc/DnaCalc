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
Scalar catalogue functions selected by `policy.py`; references, external or volatile functions, host facts, paths,
URLs and request formula text are refused before Excel runs, typed `refused`, and logged as `refused-kind`.
Limits: 250,000 calls and 32 arguments per group; the v1 queue keeps 20,000 calls and 16 MiB per request. File
captures, VBA, add-ins and XLL, RTD, events, iteration and pivots still go through `tools/excel-session` (the job
route); they are not part of this service.

## Running it (Windows)
```powershell
py -3 tools/excel-service/touchstone/broker.py start --hosts dna-firehorse dna-koderbot `
    --local-queue "$HOME\excel-queue" --relay-v1-queue "~/excel-queue"
py -3 tools/excel-service/touchstone/broker.py status | stats --since 24h | waits --since 24h | stop
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
window and checked by PID and start time. A missed batch deadline (5 s + 0.1 ms per call) kills only that Excel;
a failed start leaves the worker down (no retry loop). `service.py stats|waits` read the same logs.

Logs: `log/YYYY-MM-DD.jsonl.gz` (one compact record per batch: worker, `env_id`, calls, cells, millisecond stages),
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
