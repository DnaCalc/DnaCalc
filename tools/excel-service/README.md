# Excel questions for research agents

Python standard library only; the client works on Linux and Windows.
From a DnaCalc checkout (or copy `client.py` onto your Python path):
```python
import sys
sys.path.insert(0, "tools/excel-service")
from client import Client, number, text, boolean, blank, error
excel = Client()  # ~/excel-queue/{in,out}; default timeout 180 seconds
reply = excel.ask("ERF.PRECISE", [[number("3e47bc15e9263272")]])
print(reply["rows"][0]["bits"], reply["excel_build"])
run = excel.consecutive_run("ERF.PRECISE", "3e47bc15e9263000", 1000)
# predicate(input_bits, typed_answer) -> bool, or None if indistinguishable
# edge = excel.bisect_edge(function, low_hex, high_hex, predicate)
```
Hex means 16 binary64 bit digits, not `float.hex()` notation. `number(1.0)` also works.
Replies contain typed rows, exact input readbacks, Excel build, machine and UTC timestamp.
New replies also retain the witness worker's calculation settings, including Application decimal/
thousands separator properties and workbook date/compatibility settings (unavailable fields stay absent).
With UseSystemSeparators enabled, those Application properties can differ from the characters Excel
renders. Keep the observed text and original settings; a current setting does not establish a
historical bank's missing context. FIXED rows with no_commas enabled need no grouping separator.
Scalar numeric formatting/text calls include FIXED, DOLLAR, TEXT, VALUE, NUMBERVALUE,
VALUETOTEXT, ASC and DBCS; this does not admit array-producing or hyperlink entries from their family.
`text(..., literal=True)` and `boolean(..., literal=True)` select direct literals;
numbers always travel through Value2 cells. `error("#DIV/0!")` and `blank()` use cells.
Helpers vary one nonnegative finite argument; `arguments=` supplies the other arguments.
Failures raise `ServiceError` with its retained `.response`; timeouts retain the request.
On this Windows clone, start and stop explicitly:
```powershell
py -3 tools/excel-service/service.py start
py -3 tools/excel-service/service.py stop
```
The loop polls local `~/excel-queue` and both Linux hosts through persistent SSH peers.
SSH must already work in batch mode. Nothing is installed or started at login.
The stop file is `C:\Work\DnaCalc\codex-runs\excel-service\stop`.
One owned Excel runs through the existing session host, with an unsaved blank baseline.
Scalar catalogue functions are selected by `policy.py`; references, external/volatile
functions, array producers, paths and request formula text are refused before Excel runs.
Limits: 20,000 calls, 32 arguments/call, 200,000 total arguments, 16 MiB/request.
The host bounds each job to 75 seconds; crashes fail that request and recycle Excel.
All questions and answers stay in `C:\Work\DnaCalc\codex-runs\excel-service\log\questions.jsonl`;
raw witness audits and session receipts stay beside them under `requests` and `session`.
No request workbook is opened or saved; AutomationSecurity is preserved without assignment.
Run `python -m unittest discover -s tools/excel-service -v` for protocol/safety tests.
`prove.py --reference <ERF.PRECISE.json> --output proof.json` runs the live edge proof.
