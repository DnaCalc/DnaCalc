"""Validate real captures and compare the debugger run with its untraced control.

This checks observed boundary traffic, not a model of Excel's implementation.
Usage: python verify.py TRACED_DIRECTORY CONTROL_DIRECTORY [--out summary.json]
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess
import zipfile
import xml.etree.ElementTree as ET


def jsonl(path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8-sig").splitlines() if line]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest().upper()


def source_matches(expected, source, revision):
    root = Path(__file__).resolve().parents[2]
    if source.exists() and digest(source) == expected:
        return True
    # A later extension of the tracer must not invalidate an older immutable capture.
    # Its exact original source must still exist at the recorded repository revision.
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        return False
    relative = source.relative_to(root).as_posix()
    old = subprocess.run(["git", "-C", str(root), "show", f"{revision}:{relative}"],
                         capture_output=True, check=False)
    if old.returncode != 0:
        return False
    # Git stores LF, while these Windows source hashes can record the checked-out CRLF.
    lf = old.stdout.replace(b"\r\n", b"\n")
    return any(hashlib.sha256(content).hexdigest().upper() == expected
               for content in (old.stdout, lf, lf.replace(b"\n", b"\r\n")))


def require(condition, description):
    if not condition:
        raise AssertionError(description)


def inspect_values(value):
    if isinstance(value, dict):
        require(not value.get("unreadable"), "Unreadable value in retained capture")
        require(not value.get("omitted"), "Truncated array in retained capture")
        require("decoding" not in value, "Unsupported signature decoding in retained capture")
        for child in value.values():
            inspect_values(child)
    elif isinstance(value, list):
        for child in value:
            inspect_values(child)


def controller(directory):
    events = jsonl(directory / "controller.jsonl")
    require(not any(x["operation"] in ("failure", "cleanup failure") for x in events), "Controller reported a failure")
    return {x["operation"]: x["value"] for x in events}


def fixture(directory):
    return [line.split("\t", 3) for line in (directory / "fixture.tsv").read_text(encoding="utf-8-sig").splitlines()]


def persistence(directory):
    package = directory / "probe.xlsx"
    if package.exists():
        with zipfile.ZipFile(package) as archive:
            sheet = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
        ns = {"s": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
        cells = {cell.attrib["r"]: {"type": cell.get("t"), "formula": cell.findtext("s:f", namespaces=ns),
                                   "cache": cell.findtext("s:v", namespaces=ns)}
                 for cell in sheet.findall(".//s:c", ns) if cell.find("s:f", ns) is not None}
        result = {"workbook_sha256": digest(package), "merge_cell_count": len(sheet.findall(".//s:mergeCell", ns)), "cells": cells}
    else:
        result = json.loads((directory / "persistence.json").read_text(encoding="utf-8-sig"))
    require(result["merge_cell_count"] == 0, "Fixture contains merged cells")
    for address, formula in {"B1": '_xll.X0.Hello("world")', "B2": '_xll.X0.Reference(A1)',
                             "B3": '_xll.X0.Caller()', "B4": '_xll.X0.Async(41)',
                             "B5": 'SUM(_xll.X0.Big())', "B6": '_xll.X0.NativeOwned()'}.items():
        require(result["cells"][address]["formula"] == formula, "Wrong persisted formula: " + address)
    return result


def boundary(directory):
    events = jsonl(directory / "boundary.jsonl")
    require([x["seq"] for x in events] == list(range(1, len(events)+1)), "Noncontiguous trace sequence")
    require(events[-1]["event"] == "detach" and events[-1]["healthy"], "Tracer did not detach cleanly")
    require(events[-1]["restore_failed"] == events[-1]["outstanding_calls"] == 0, "Incomplete breakpoint cleanup or calls")
    require(not any(x["event"] in ("breakpoint_error", "debugger_error", "read_error", "second_chance_exception", "process_exit") for x in events), "Tracer error")
    enters = {x["call"]: x for x in events if x["event"] == "enter"}
    leaves = {x["call"]: x for x in events if x["event"] == "leave"}
    require(enters.keys() == leaves.keys(), "Calls lack entries or returns")
    for key, entry in enters.items():
        result = leaves[key]
        require(entry["thread"] == result["thread"] and entry["entry"] == result["entry"], "Entry/return identity mismatch")
        require(entry["seq"] < result["seq"] and result["elapsed_qpc"] >= 0, "Invalid call ordering or timing")
        inspect_values(entry)
        inspect_values(result)

    return events, enters, leaves


def verify(traced, control):
    events, enters, leaves = boundary(traced)

    env = json.loads((traced / "environment.json").read_text(encoding="utf-8-sig"))
    control_env = json.loads((control / "environment.json").read_text(encoding="utf-8-sig"))
    for key in ("excel_file_version", "xll_sha256", "fixture_dll_sha256", "fixture_source_sha256", "fixture_project_sha256", "native_source_sha256", "native_xll_sha256"):
        require(env[key] == control_env[key], "Control differs: " + key)
    source_root = Path(__file__).resolve().parent
    source_publication = traced.parent / "source-publication.json"
    revision = (json.loads(source_publication.read_text(encoding="utf-8"))["source_revision"]
                if source_publication.exists() else env["repository_commit"])
    for field, source in (("fixture_source_sha256", "fixture/Probe.cs"), ("fixture_project_sha256", "fixture/X0.csproj"), ("tracer_source_sha256", "native/trace.cpp"), ("native_source_sha256", "native/owned.cpp")):
        require(source_matches(env[field], source_root / source, revision), "Capture source is not retained: " + source)
    require(env["pid"] != 3144 and env["pid"] not in env["preexisting_excel_pids"], "PID ownership not established")

    actual = controller(traced)
    expected = controller(control)
    saved = persistence(traced)
    control_saved = persistence(control)
    for operation, value in {
        "RegisterXLL": True, "RegisterXLL native": True, "Value2 B1": "Hello, world", "Value2 B2": 7,
        "Value2 B4": 42, "Value2 B5": 8386560, "Value2 D1": 2, "Value2 D32": 64,
        "Application.Run X0.Queue": True, "Application.Run X0.Context": True,
        "ExecuteExcel4Macro UNREGISTER(path)": True,
        "Value2 B6": "native owned", "ExecuteExcel4Macro UNREGISTER(native)": True,
    }.items():
        require(actual.get(operation) == expected.get(operation) == value, "Wrong or different outcome: " + operation)
    caller = lambda value: re.sub(r"sheet=\d+", "sheet=<session>", value)
    require(caller(actual["Value2 B3"]) == caller(expected["Value2 B3"]) == "ExcelReference sheet=<session> rows=2:2 cols=1:1", "Wrong caller cell")

    registrations = [x for x in events if x["event"] == "registered_entry"]
    signatures = {x["entry"].split(":", 1)[1]: x["signature"] for x in registrations if ":X0." in x["entry"]}
    for name, signature in {"X0.Hello": "QD%", "X0.ThreadSafe": "QE$", "X0.Reference": "QU#", "X0.Caller": "Q#", "X0.Big": "Q", "X0.Async": ">EX"}.items():
        require(signatures.get(name) == signature, "Unexpected registration: " + name)
    auto_open = [x for x in enters.values() if x["entry"] == "X0-AddIn64.xll!xlAutoOpen"]
    require(len(auto_open) == 1, "Expected one load")
    main_thread = auto_open[0]["thread"]
    worker_calls = [x for x in enters.values() if x["entry"].endswith(":X0.ThreadSafe")]
    worker_threads = sorted({x["thread"] for x in worker_calls})
    require(any(thread != main_thread for thread in worker_threads), "No thread-safe call on a calculation worker")
    for entry in worker_calls:
        require(leaves[entry["call"]]["result"]["value"] == entry["args"][0]["value"]*2, "Wrong worker result")
    big_returns = [x for x in leaves.values() if x["entry"].endswith(":X0.Big")]
    require(big_returns, "Mixed-array function was not observed returning")
    for result in big_returns:
        array = result["result"]["value"]
        require((array["rows"], array["columns"]) == (1, 4096), "Wrong mixed-array shape")
        require(array["items"][0]["value"] == "marker" and
                [x["value"] for x in array["items"][1:]] == list(range(1, 4096)), "Wrong live mixed-array values")
    owned_returns = [x for x in leaves.values() if x["entry"].endswith(":X0.NativeOwned")]
    frees = [x for x in enters.values() if x["entry"] == "NativeOwner64.xll!xlAutoFree12"]
    require(owned_returns and len(frees) == len(owned_returns), "Native owned result was not freed exactly once")
    for result in owned_returns:
        require(result["result"]["type"] == 0x4002, "Missing DLLFree on native string")
        free = [x for x in frees if x["oper_address"] == result["rax"] and x["seq"] > result["seq"]]
        require(len(free) == 1 and free[0]["thread"] == result["thread"], "Wrong free pointer or thread")
    callbacks = [x for x in enters.values() if x["entry"].endswith("!MdCallBack12")]
    async_returns = [x for x in callbacks if x["function"] == 16400]
    require(async_returns and all(leaves[x["call"]]["return_code"] == 0 for x in async_returns), "Async return not accepted")
    require(any(x["thread"] != main_thread for x in async_returns), "Async return not on a background thread")
    require(any(x["entry"].endswith("!xlAutoClose") for x in enters.values()), "No native xlAutoClose observed")

    fixture_rows = fixture(traced)
    require(any(x[2] == "Queue executed" for x in fixture_rows), "Queued macro did not execute")
    for fn in (16393, 16385, 89):
        require(any(x[2] == f"Outside {fn}" and x[3].startswith("XlReturnFailed") for x in fixture_rows), "Missing outside-context result")
    sink_calls = [x for x in enters.values() if x["entry"] == "COM!Invoke" and x.get("object_label", "").startswith("AppEvents sink")]
    require(any(x["dispid"] == 0x61c and len(x["rgvarg"]) == 2 for x in sink_calls), "No raw SheetChange invocation")
    require(any(x["entry"] == "COM!FindConnectionPoint" for x in enters.values()), "No connection-point discovery")
    require(any(x["entry"] == "COM!Advise" for x in enters.values()), "No event subscription")
    callback_codes = Counter((x["function"], leaves[x["call"]]["return_code"]) for x in callbacks)
    member_names = {}
    for x in enters.values():
        if x["entry"] == "COM!GetIDsOfNames":
            for name, dispid in zip(x["names"], leaves[x["call"]].get("dispids", [])):
                member_names[(x["object"], dispid)] = name
    com_members = Counter(member_names.get((x["object"], x["dispid"]), x.get("member") or "dispid:"+str(x["dispid"])) for x in enters.values() if x["entry"] == "COM!Invoke" and not x.get("object_label", "").startswith("AppEvents sink"))
    managed_members = Counter(member_names.get((x["object"], x["dispid"]), x.get("member") or "dispid:"+str(x["dispid"])) for x in enters.values() if x["entry"] == "COM!Invoke" and
                              not x.get("object_label", "").startswith("AppEvents sink") and
                              ("unmapped" in x["from"] or x["from"].get("module", "").lower() in ("clr.dll", "mscorlib.ni.dll")))
    return {
        "status": "verified", "excel_file_version": env["excel_file_version"],
        "trace_sha256": digest(traced / "boundary.jsonl"), "trace_records": len(events), "paired_calls": len(enters),
        "c_api_calls": len(callbacks), "main_thread": main_thread, "thread_safe_threads": worker_threads,
        "thread_safe_calls": len(worker_calls), "async_returns": len(async_returns), "xlAutoFree12_calls": len(frees),
        "managed_array_return_types": sorted({x["result"]["type"] for x in big_returns}),
        "registration_signatures": signatures,
        "callback_return_codes": [{"function": fn, "return_code": rc, "calls": count} for (fn, rc), count in sorted(callback_codes.items())],
        "com_members": dict(sorted(com_members.items())),
        "com_members_managed_origin": dict(sorted(managed_members.items())),
        "com_origin_note": "Managed/JIT origin is an attribution inference; all observed COM traffic also includes the external controller.",
        "app_events": dict(Counter(str(x["dispid"]) for x in sink_calls)),
        "control_outcomes_match": True,
        "saved_xlsx": {"traced_sha256": saved["workbook_sha256"], "control_sha256": control_saved["workbook_sha256"],
                       "merged_cells": 0, "xll_prefix_checked_cells": ["B1", "B2", "B3", "B4", "B5", "B6"]},
        "timing": "QPC includes debugger pauses; no uninstrumented latency or scheduling claim",
    }


def verify_x1(excel, excel_control, host, host_control):
    cases_path = Path(__file__).resolve().parent / "fixture/x1-cases.json"
    cases = json.loads(cases_path.read_text(encoding="utf-8"))
    expected = {case["formula"]: case["value"] for case in cases}
    require(len(expected) == 21, "X1 case denominator changed")
    environments = [json.loads((p / "environment.json").read_text(encoding="utf-8-sig"))
                    for p in (excel, excel_control, host, host_control)]
    for env in environments:
        require(env["native_xll_sha256"] == environments[0]["native_xll_sha256"], "Different fixture binaries")
        require(env["cases_sha256"] == digest(cases_path), "Case file differs from capture")
        root = Path(__file__).resolve().parents[2]
        for source, sha in env["source_files"].items():
            require(source_matches(sha, root / source, env["repository_commit"]), "Source not retained: " + source)
    require(environments[0]["excel_file_version"] == environments[1]["excel_file_version"], "Different Excel builds")
    require(environments[2]["executable_sha256"] == environments[3]["executable_sha256"], "Different host binaries")
    require(environments[2]["executable_version"].startswith("16."), "Missing host version resource")
    for directory, env in zip((excel, excel_control), environments):
        require(env["pid"] != 3144 and env["pid"] not in env["preexisting_excel_pids"], "Unowned Excel PID")
        observations = controller(directory)
        require(observations["AutomationSecurity unchanged"] is True, "Security setting changed")
        actual = {k: v for k, v in observations.items() if k.startswith("=")}
        require(actual == expected, "Excel formula outcomes differ from the X1 cases")
        require(observations["raw =MyEcho(#N/A)"] == -2146826246, "Missing raw Excel #N/A error")
        for name in ("MyNonfiniteValue", "MyNonfinitePointer", "MyNonfiniteOper"):
            require(observations[f"raw ={name}()"] == -2146826252, "Missing raw Excel #NUM error")
    for directory in (host, host_control):
        output = json.loads((directory / "host.stdout").read_text(encoding="utf-8-sig"))
        require({r["formula"]: r["value"] for r in output["results"]} == expected, "DNA Calc formula outcomes differ")
        require(output["registrations"] == 18 and output["duplicate_use_count"] == 2, "Registration use count differs")
        require(output["live_callback_allocations"] == 0, "Host callback storage leaked")
        require(output["closed_registration"] == "#NAME?", "Old registry still callable after close")
        require(not (directory / "host.stderr").read_text(encoding="utf-8-sig").strip(), "Host reported diagnostics")

    excel_events, excel_enters, excel_leaves = boundary(excel)
    host_events, host_enters, host_leaves = boundary(host)
    counts = []
    ownership = []
    signatures = []
    for events, enters, leaves in ((excel_events, excel_enters, excel_leaves), (host_events, host_enters, host_leaves)):
        callback_counts = Counter((x["function"], x["return_code"]) for x in leaves.values() if "return_code" in x)
        counts.append(callback_counts)
        for function in (16393, 16385, 89, 16384):
            require(callback_counts[function, 32] == 1, f"Missing outside-context failure for {function}")
        registrations = [x for x in events if x["event"] == "registered_entry"]
        signatures.append(Counter((r["entry"].split(":", 1)[1], r["signature"]) for r in registrations))
        require(len(registrations) == 19, "Registration callback count differs")
        for entry in enters.values():
            if entry["entry"].endswith("!XLCallVer"):
                require(int(leaves[entry["call"]]["rax"], 16) & 0xffffffff == 3072, "XLCallVer differs")
            if entry["entry"].endswith("!LPenHelper"):
                returned = leaves[entry["call"]]
                require(entry["function"] == 16398 and returned["output_first_long"] == 0, "LPenHelper edit mode differs")
                require(int(returned["rax"], 16) & 0xffffffff == 0, "LPenHelper return differs")
        require(any(e["entry"].endswith("!XLCallVer") for e in enters.values()), "XLCallVer not exercised")
        require(any(e["entry"].endswith("!LPenHelper") for e in enters.values()), "LPenHelper not exercised")
        owned = [x for x in leaves.values() if x["entry"].endswith(":MyOwned")]
        frees = [x for x in enters.values() if x["entry"].endswith("!xlAutoFree12")]
        require(len(owned) == len(frees) == 1, "DLLFree ownership was not exercised exactly once")
        require(owned[0]["result"]["type"] == 0x4002, "Owned string lost DLLFree flag")
        require(owned[0]["rax"] == frees[0]["oper_address"] and owned[0]["thread"] == frees[0]["thread"], "xlAutoFree12 pointer/thread differs")
        ownership.append({"returns": len(owned), "frees": len(frees), "same_pointer_and_thread": True})
        dynamic = {x["entry"].rsplit(":", 1)[-1]: x["result"] for x in leaves.values() if ":MyDynamic" in x["entry"]}
        require(dynamic == {"MyDynamicA": 41, "MyDynamicB": 42}, "Registration address was not frozen")
    require(counts[0] == counts[1], "C API function/return-code counts differ between Excel and DNA Calc")
    require(signatures[0] == signatures[1], "Registration signatures differ")
    return {
        "status": "verified X1 fixture boundary", "formula_cases": len(expected), "formula_matches_each_host": len(expected),
        "excel_file_version": environments[0]["excel_file_version"], "host_file_version": environments[2]["executable_version"],
        "same_fixture_sha256": environments[0]["native_xll_sha256"],
        "excel_trace_records": len(excel_events), "host_trace_records": len(host_events),
        "c_api_calls_each_host": sum(counts[0].values()), "callback_return_codes": [
            {"function": fn, "return_code": rc, "calls_each_host": n} for (fn, rc), n in sorted(counts[0].items())],
        "ownership_excel_and_host": ownership, "registration_time_address_lookup": True,
        "controls_match": True, "timing": "debugger timing; no uninstrumented scheduling or latency claim",
        "scope": "OneCalc controller in a minimal native executable; no Excel-DNA, COM object model or worker scheduling claim",
        "async_result_operand": "Excel direct callback leaves type zero; DNA Calc supplies the documented Boolean. Callback codes and published values match; this raw output difference remains recorded.",
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("traced", type=Path)
    parser.add_argument("control", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--host", type=Path)
    parser.add_argument("--host-control", type=Path)
    args = parser.parse_args()
    if args.host:
        parser.error("--host requires --host-control") if args.host_control is None else None
        result = verify_x1(args.traced, args.control, args.host, args.host_control)
    else:
        result = verify(args.traced, args.control)
    rendered = json.dumps(result, indent=2) + "\n"
    if args.out:
        args.out.write_text(rendered, encoding="utf-8")
    print(rendered)
