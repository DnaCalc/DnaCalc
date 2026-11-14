"""Small MIT fixture for the original xlOil Python plugin; no framework changes."""
import xloil
import sys
from pathlib import Path


@xloil.func(name="X3.PyAdd", macro=False)
def add(a: float, b: float = 2) -> float:
    return a + b


@xloil.func(name="X3.PyHello", macro=False)
def hello(name: str) -> str:
    return "Hello, " + name


@xloil.func(name="X3.PyArray", threaded=True)
def array(n: int):
    return [[i, i * i] for i in range(n)]


# X4 deliberately keeps Python callbacks in the original plugin. These are
# observations of COM objects and delivered events, not stored worksheet answers.
_changes = []
_all_changes = []
_details = []
_calculations = 0
_last_range = None
_queued = None


def _describe(value):
    if hasattr(value, "tolist"):
        value = value.tolist()
    return repr(value)


def _sheet_change(name, target):
    global _last_range, _queued
    # What the handler reads of its Target (address, shape, value) before anything else; a
    # failing read is recorded by its exception type, so both hosts' answers stay comparable.
    try:
        detail = "|".join((name, target.address(local=True), str(target.shape),
                           _describe(target.value)))
    except Exception as error:
        detail = name + "|" + type(error).__name__
    _details.append(detail)
    address = target.address(local=True)
    _all_changes.append(name + "|" + address)
    if address == "A1":
        _changes.append(name + "|" + address)
        _last_range = target
        if _queued is None:
            # AppEvents enter outside XLL context. The unmodified plugin uses
            # its registered xloRunInXLLContext command through Application.Run.
            _queued = xloil.call_async("SUM", 20, 22)


xloil.event.SheetChange += _sheet_change


def _startup_calculate():
    global _calculations
    _calculations += 1
    # The importer pauses Python events through scan_module. A delivered
    # calculation event after that module appears acknowledges event resumption.
    ready = Path(__file__).with_name("x4-startup-ready")
    if "xloil.excelfuncs" in sys.modules and not ready.exists():
        ready.write_text("AfterCalculate delivered\n")


xloil.event.AfterCalculate += _startup_calculate


if __import__("os").environ.get("DNA_XLL_RTD_TESTS"):
    import asyncio

    @xloil.func(name="X5.Rtd", threaded=False)
    async def rtd_sequence():
        yield 1
        await asyncio.sleep(0.25)
        yield 42


@xloil.func(name="X4.PyCom")
def com_properties() -> str:
    app = xloil.app()
    return "|".join((app.active_workbook.name, app.active_worksheet.name,
                     str(len(app.workbooks)), str(app.enable_events)))


@xloil.func(name="X4.PyEvents")
def events() -> str:
    return ";".join(_changes)


@xloil.func(name="X4.PyTargets")
def all_event_targets() -> str:
    return ";".join(_all_changes)


@xloil.func(name="X4.PyDetails")
def event_details() -> str:
    # The case formula's own cell (B3) is left out: when its SheetChange arrives relative to
    # its own calculation is a separate observation (X4.PyTargets keeps every target).
    return ";".join(d for d in _details if "|B3|" not in d and not d.endswith("|B3"))


@xloil.func(name="X4.PyCalculateCount")
def calculation_count() -> int:
    return _calculations


@xloil.func(name="X4.PyBounds")
def bounds() -> str:
    if _last_range is None:
        return "no delivered range"
    return "|".join((str(_last_range.row), str(_last_range.column),
                     str(_last_range.shape), _last_range.parent.name,
                     _last_range.address(style="$a$1", local=True)))


@xloil.func(name="X4.PyCall")
def call_in_context() -> float:
    return xloil.call("SUM", 19, 23)


@xloil.func(name="X4.PyQueued")
def queued_result() -> float:
    return _queued.result()


@xloil.func(name="X4.PyRelease")
def release_objects() -> int:
    global _last_range, _queued
    _last_range = None
    _queued = None
    return len(_changes)
