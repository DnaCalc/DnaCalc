"""Validate real captures and compare the debugger run with its untraced control.

This checks observed boundary traffic, not a model of Excel's implementation.
Usage: python verify.py TRACED_DIRECTORY CONTROL_DIRECTORY [--out summary.json]
"""
import argparse
from collections import Counter
from datetime import datetime
from functools import lru_cache
from itertools import zip_longest
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
import zipfile
import xml.etree.ElementTree as ET

rewritten_source_bindings = {}
excel_session_directory = None
session_ownership_bindings = {}


# The executables a host capture runs in: DNA Calc's engine harness and OneCalc's formula session.
HOST_MODULES = ("dna-calc-desktop.exe", "dna-onecalc-desktop.exe")

def content_matches(expected, content):
    lf = content.replace(b"\r\n", b"\n")
    return any(hashlib.sha256(value).hexdigest().upper() == expected
               for value in (content, lf, lf.replace(b"\n", b"\r\n")))


@lru_cache(maxsize=None)
def revision_exists(root, revision):
    return subprocess.run(["git", "-C", root, "cat-file", "-e", revision + "^{commit}"],
                          capture_output=True, check=False).returncode == 0


@lru_cache(maxsize=None)
def rewritten_source(root, relative, expected):
    # Only the same path, only retained commits, and at most 256 content versions.
    history = subprocess.run(["git", "-C", root, "log", "--all", "-n", "256", "--format=%H", "--", relative],
                             capture_output=True, check=True, text=True).stdout.splitlines()
    for index, candidate in enumerate(history):
        blob = subprocess.run(["git", "-C", root, "show", candidate + ":" + relative], capture_output=True, check=False)
        if blob.returncode == 0 and content_matches(expected, blob.stdout):
            return candidate, index + 1
    return None


def ident(row):
    """The identity of one RTD admission, input read or retirement in the host's evidence: the
    topic and lease (dc-snz.23.2), or the provider and subscription of earlier captures."""
    if "topic" in row:
        return (row["topic"], row["lease"])
    return (row["provider"], row["subscription"])


def ident_fields(row):
    if "topic" in row:
        return {"topic": row["topic"], "lease": row["lease"]}
    return {"provider": row["provider"], "subscription": row["subscription"]}


def jsonl(path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8-sig").splitlines() if line]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest().upper()


@lru_cache(maxsize=None)
def source_matches(expected, source, revision):
    root = Path(__file__).resolve().parents[2]
    # A later extension of the tracer must not invalidate an older immutable capture.
    # Its exact original source must still exist at the recorded repository revision.
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        return False
    relative = source.relative_to(root).as_posix()
    old = subprocess.run(["git", "-C", str(root), "show", f"{revision}:{relative}"],
                         capture_output=True, check=False)
    if old.returncode == 0:
        return content_matches(expected, old.stdout)
    if revision_exists(str(root), revision):
        return False
    match = rewritten_source(str(root), relative, expected)
    if match is None:
        return False
    replacement, inspected = match
    rewritten_source_bindings[(relative, expected, revision)] = {
        "path": relative, "sha256": expected, "recorded_revision": revision,
        "retained_rewritten_revision": replacement, "same_path_versions_inspected": inspected,
        "normalization": "LF/CRLF only; exact recorded SHA-256 required",
    }
    return True


def require(condition, description):
    if not condition:
        raise AssertionError(description)


@lru_cache(maxsize=None)
def session_receipts(session):
    receipts = {}
    for path in (session / "completed").glob("*/receipt.json"):
        receipt = json.loads(path.read_text(encoding="utf-8-sig"))
        directory = receipt.get("job", {}).get("parameters", {}).get("RunDirectory")
        if directory:
            receipts.setdefault(Path(directory).resolve(), []).append(receipt)
    return receipts


def verify_excel_ownership(directory, env, session=None, fresh=True):
    """Use the runner's launch evidence or an explicitly supplied session ledger."""
    require(env["pid"] != 3144, "Protected Excel PID")
    session = session if session is not None else excel_session_directory
    if session is None:
        require(env["pid"] not in env["preexisting_excel_pids"], "Unowned Excel PID")
        return
    receipts = session_receipts(session).get(directory.resolve(), [])
    retained = directory / "session-receipt.json"
    if len(receipts) > 1 and not fresh and retained.is_file():
        # A run directory name reused by an earlier attempt: the receipt the runner copied beside the capture names its job.
        job = json.loads(retained.read_text(encoding="utf-8-sig"))["id"]
        receipts = [r for r in receipts if r["id"] == job]
    require(len(receipts) == 1, "Missing or ambiguous owned-session receipt")
    receipt = receipts[0]
    require(receipt["success"] and (receipt["job"]["fresh_required"] or not fresh), "Session capture did not complete in a fresh process")
    owned = receipt["excel"]
    require(owned["pid"] == env["pid"] and owned["start_ticks"] > 0
            and "/restore" not in owned["command_line"].lower(), "Session Excel identity differs")
    launch = json.loads((session / "launches" / f'{owned["launch"]}.json').read_text(encoding="utf-8-sig"))
    require(launch["excel"] == owned, "Session PID/start-time tuple differs from its launch")
    attempts = [e["data"] for e in jsonl(session / "events.jsonl")
                if e["kind"] == "launch-attempt" and e["data"]["launch"] == owned["launch"]]
    require(len(attempts) == 1 and owned["pid"] not in attempts[0]["preexisting_pids"], "Session Excel was present before activation")
    host_sha = receipt["host_script_sha256"].upper()
    require(host_sha == launch["host_script_sha256"].upper()
            and digest(session / "launches" / launch["host_script"]) == host_sha,
            "Session host source differs from launch evidence")
    root = Path(__file__).resolve().parents[2]
    require(source_matches(host_sha, root / "tools/excel-session/host.ps1", env["repository_commit"]),
            "Session host source is not retained at the capture revision")
    require(receipt["execution_script_sha256"].upper() == env["runner_sha256"]
            and receipt["script_sha256"].upper() == env["runner_sha256"], "Session runner hash differs")
    require(receipt["preserve_automation_security"]
            and receipt["baseline_before"]["AutomationSecurity"] == env["automation_security_before"]
            and receipt["baseline"]["AutomationSecurity"] == env["automation_security_before"],
            "Session AutomationSecurity baseline differs")
    when = datetime.fromisoformat(env["captured_utc"])
    require(datetime.fromisoformat(receipt["started_utc"]) <= when
            <= datetime.fromisoformat(receipt["finished_utc"]), "Capture time lies outside its session job")
    session_ownership_bindings[str(directory.resolve())] = {
        "pid": owned["pid"], "start_ticks": owned["start_ticks"], "launch": owned["launch"],
        "job": receipt["id"], "host_script_sha256": host_sha,
        "scope": "Exact successful fresh-job receipt, launch tuple, pre-activation inventory, retained host/runner sources, security baseline and capture interval checked; raw environment remains unchanged.",
    }


def verify_sources(environments, publication_path=None):
    root = Path(__file__).resolve().parents[2]
    publications = json.loads(publication_path.read_text(encoding="utf-8-sig"))["publications"] if publication_path else []
    def retained(sha, source, env):
        require(not Path(source).is_absolute() and ".." not in Path(source).parts, "Invalid source path")
        if source_matches(sha, root / source, env["repository_commit"]):
            return True
        return any(p["source_files"].get(source) == sha and source_matches(sha, root / source, p["revision"])
                   for p in publications)
    for env in environments:
        require(retained(env["tracer_source_sha256"], "crates/calc-xll-reference/native/trace.cpp", env), "Tracer source not retained")
        require(retained(env["runner_sha256"], "crates/calc-xll-reference/run.ps1", env), "Runner source not retained")
        for source, sha in env["source_files"].items():
            require(retained(sha, source, env), "Source not retained: " + source)
    return publications


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
    require(not any(x["event"] in ("breakpoint_error", "debugger_error", "read_error", "unrecognized_breakpoint", "second_chance_exception", "process_exit") for x in events), "Tracer error")
    enters = {x["call"]: x for x in events if x["event"] == "enter"}
    leaves = {x["call"]: x for x in events if x["event"] == "leave"}
    require(enters.keys() == leaves.keys(), "Calls lack entries or returns")
    for key, entry in enters.items():
        result = leaves[key]
        require(entry["thread"] == result["thread"] and entry["entry"] == result["entry"], "Entry/return identity mismatch")
        require(entry["seq"] < result["seq"] and result["elapsed_qpc"] >= 0, "Invalid call ordering or timing")
        inspect_values(entry)
        inspect_values(result)

    arity = {"object1":1,"bstr1":1,"int1":1,"bool1":1,"object2":2,"bstr2":2,
             "rtdstart":2,"rtdconnect":4,"rtdrefresh":2,"rtddisconnect":1,"rtdinterval":1,"rtdterminate":0,"rtdnotify":0}
    for forwarded in (e for e in events if e["event"] == "com_tail_forward"):
        require(forwarded["call"] in enters, "COM forwarding has no originating call")
        origin = enters[forwarded["call"]]
        require(origin["seq"] < forwarded["seq"] < leaves[origin["call"]]["seq"]
                and forwarded["thread"] == origin["thread"] and forwarded["entry"] == origin["entry"], "COM forwarding left its physical call")
        require(forwarded["stack"] == origin["stack"] and forwarded["return"] == origin["return"], "COM forwarding left its caller frame")
        require(("origin_entry_address" in forwarded) == ("same_entry" in forwarded), "Incomplete COM forwarding address diagnostics")
        if "origin_entry_address" in forwarded:
            require(forwarded["origin_entry_address"] == origin["entry_address"]
                    and forwarded["same_entry"] == (forwarded["entry_address"] == origin["entry_address"]), "COM forwarding entry-address identity differs")
        require(forwarded["non_this_arguments"] == origin["non_this_arguments"]
                and len(forwarded["non_this_arguments"]) == arity.get(origin["result_abi"])
                and int(forwarded["object"],16) != 0 and int(origin["object"],16) != 0, "COM forwarding arguments or interface identity are invalid")

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
    require(len(expected) == 289, "X1 case denominator changed")
    environments = [json.loads((p / "environment.json").read_text(encoding="utf-8-sig"))
                    for p in (excel, excel_control, host, host_control)]
    require(len({env["repository_commit"] for env in environments}) == 1, "Different capture source revisions")
    for env in environments:
        require(env["native_xll_sha256"] == environments[0]["native_xll_sha256"], "Different fixture binaries")
        require(env["cases_sha256"] == digest(cases_path), "Case file differs from capture")
        root = Path(__file__).resolve().parents[2]
        require(source_matches(env["tracer_source_sha256"], Path(__file__).resolve().parent / "native/trace.cpp", env["repository_commit"]), "Tracer source not retained")
        for source, sha in env["source_files"].items():
            require(source_matches(sha, root / source, env["repository_commit"]), "Source not retained: " + source)
    require(environments[0]["excel_file_version"] == environments[1]["excel_file_version"], "Different Excel builds")
    require(environments[2]["executable_sha256"] == environments[3]["executable_sha256"], "Different host binaries")
    require(environments[2]["shim_sha256"] == environments[3]["shim_sha256"], "Different companion DLL binaries")
    require(environments[0]["tracer_sha256"] == environments[2]["tracer_sha256"], "Different tracer binaries")
    require(environments[2]["executable_version"].startswith("16."), "Missing host version resource")
    for directory, env in zip((excel, excel_control), environments):
        if excel_session_directory is not None:
            # An ordinary (reused-process) session job: the receipt still binds PID, start time and launch.
            verify_excel_ownership(directory, env, fresh=False)
        else:
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
        require(output["registrations"] == 48 and output["duplicate_use_count"] == 2, "Registration use count differs")
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
        require(len(registrations) == 48, "Registration callback count differs")
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
    sequence = [[(x["function"], x["return_code"]) for x in events
                 if x["event"] == "leave" and "return_code" in x] for events in (excel_events, host_events)]
    require(sequence[0] == sequence[1], "Ordered C API callback/return-code sequences differ")
    require(signatures[0] == signatures[1], "Registration signatures differ")
    # What xlFree leaves in the caller's operands (dc-6332.10: Excel nulls the pointer and sets
    # xltypeMissing). Traces before the tracer recorded args_after have none to compare.
    freed = [[[normalize_x2(defined_boundary(arg)) for arg in x["args_after"]] for x in events
              if x["event"] == "leave" and x.get("function") == 16384 and "args_after" in x]
             for events in (excel_events, host_events)]
    if freed[0] or freed[1]:
        require(freed[0] == freed[1], "Operands after xlFree differ between Excel and DNA Calc")
    # Each callback's context: the export that was running when it was made (traces with "within").
    within = [[(x["function"], x["within"].split("!", 1)[-1] if x["within"] else None) for x in enters.values()
               if x["entry"].endswith("!MdCallBack12") and "within" in x]
              for enters in (excel_enters, host_enters)]
    if within[0] or within[1]:
        require(within[0] == within[1], "Callback contexts (the running export) differ between Excel and DNA Calc")
    # C.1: the same verification reported as the four lanes (each check above already required
    # every row to match, so a lane here is all pass or the verification failed).
    lanes = ([{"lane": "value_cases", "outcome": "pass"}] * len(expected)
             + [{"lane": "ordered_boundary_cases", "outcome": "pass"}] * len(sequence[0])
             + [{"lane": "lifecycle_ownership_cases", "outcome": "pass"}] * (len(freed[0]) + sum(o["frees"] for o in ownership[:1])))
    return {
        "status": "verified X1 fixture boundary", "formula_cases": len(expected), "formula_matches_each_host": len(expected),
        "counts": four_counts(lanes),
        "manifest": comparison_manifest(environments[0], jsonl(excel / "controller.jsonl"), cases, environments[2:]),
        "xlfree_operands_after_compared": len(freed[0]), "callback_contexts_compared": len(within[0]),
        "source_revision": environments[0]["repository_commit"],
        "ordered_callback_sequence_matches": True,
        "trace_sha256": {"excel": digest(excel / "boundary.jsonl"), "dna_calc": digest(host / "boundary.jsonl")},
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


def defined_boundary(value):
    """Check the new BigData decoder against its retained bytes before comparing fields.

    cbData is a signed 32-bit C long. Its following padding is retained in raw
    records, while every declared field remains part of the defined comparison.
    Older captures keep their original decoding and comparison behavior.
    """
    if isinstance(value, dict):
        if "abi_storage" in value:
            require(set(value) == {"type", "value", "abi_storage"}, "Unexpected BigData operand fields")
            require(isinstance(value["type"], int) and value["type"] & 0xfff == 2050, "BigData storage on another operand type")
            storage = value["abi_storage"]
            require(set(storage) == {"layout", "raw32", "padding_after_cbData"}
                    and storage["layout"] == "xloper12-bigdata-long32/1", "Unknown BigData storage layout")
            require(isinstance(storage["raw32"], str) and re.fullmatch(r"[0-9a-f]{64}", storage["raw32"]), "Invalid BigData raw operand")
            raw = bytes.fromhex(storage["raw32"])
            fields = value["value"]
            require(set(fields) == {"opaque_handle", "byte_count"}, "Unexpected BigData value fields")
            require(isinstance(fields["opaque_handle"], str) and re.fullmatch(r"0x[0-9a-f]+", fields["opaque_handle"]), "Invalid raw BigData identity")
            require(int(fields["opaque_handle"], 16) == struct.unpack_from("<Q", raw)[0], "BigData pointer differs from raw bytes")
            require(type(fields["byte_count"]) is int and fields["byte_count"] == struct.unpack_from("<i", raw, 8)[0], "BigData cbData differs from raw bytes")
            require(value["type"] == struct.unpack_from("<I", raw, 24)[0], "BigData type differs from raw bytes")
            require(storage["padding_after_cbData"] == raw[12:16].hex(), "BigData padding differs from raw bytes")
            return {"type": value["type"], "value": defined_boundary(fields)}
        return {key: defined_boundary(child) for key, child in value.items()}
    if isinstance(value, list):
        return [defined_boundary(child) for child in value]
    return value


def normalize_x2(value):
    """Normalize only process identities: sheet IDs, handles, GUID names and XLL paths.

    Return-code/type/ownership bits, rectangles, array shapes and values remain exact.
    """
    if isinstance(value, dict):
        if "abi_storage" in value:
            # Raw storage duplicates the opaque handle in its first eight bytes.
            # Validate that binding before masking identity; keep all other bytes,
            # including padding. Permit a second normalization of our own output.
            pointer = value.get("value", {}).get("opaque_handle")
            if pointer != "<handle>":
                defined_boundary(value)
            else:
                require(value["abi_storage"]["raw32"].startswith("0" * 16),
                        "Normalized BigData still contains a raw identity")
                defined_boundary({**value, "value": {**value["value"], "opaque_handle": "0x0"}})
            storage = dict(value["abi_storage"])
            storage["raw32"] = "0" * 16 + storage["raw32"][16:]
            value = {**value, "abi_storage": storage}
        return {key: ("<sheet>" if key == "sheet" else "<handle>" if key == "opaque_handle"
                      else normalize_x2(child)) for key, child in value.items()}
    if isinstance(value, list):
        return [normalize_x2(child) for child in value]
    if isinstance(value, str):
        value = re.sub(r"sheet=-?\d+", "sheet=<sheet>", value)
        value = re.sub(r"(SyncMacro|RegistrationInfo)_[a-f0-9]{32}", r"\1_<guid>", value)
        value = re.sub(r"RegistrationInfo_[a-f0-9]{28,31}(?![a-f0-9])", "RegistrationInfo_<guid>", value)
        if "\\" in value and value.lower().endswith(".xll"):
            value = value.replace("\\", "/").rsplit("/", 1)[-1]
        return value
    return value


def invocation_record(trace, entry, defined=True):
    events, enters, leaves = trace
    returned = leaves[entry["call"]]
    normalize = lambda value: normalize_x2(defined_boundary(value) if defined else value)
    callbacks = [[x["function"], normalize(x["args"]), leaves[x["call"]]["return_code"],
                  None if defined and x["function"] == 16384 else normalize(leaves[x["call"]].get("result"))]
                 for x in enters.values() if x["entry"].endswith("!MdCallBack12")
                 and x["thread"] == entry["thread"] and entry["seq"] < x["seq"]
                 and leaves[x["call"]]["seq"] < returned["seq"]]
    return {"arguments": normalize(entry.get("args")),
            "result": normalize(returned.get("result")), "callbacks": callbacks}


def x2_invocation(trace, name):
    entry = next((x for x in trace[1].values() if x["entry"].endswith(":" + name)), None)
    require(entry is not None, "No native invocation for " + name)
    return invocation_record(trace, entry)


def all_invocations(trace, name, defined=True):
    return [invocation_record(trace, x, defined) for x in trace[1].values() if x["entry"].endswith(":" + name)]


def framework_callbacks(trace, teardown=False):
    """Keep every callback before close; registration IDs and windows are identities.

    xlFree has no defined output operand. Its raw buffer stays in the trace; the
    operands it leaves (args_after, traces from dc-6332.10 on) are compared when both
    traces record them. The close controllers use UNREGISTER(path) in Excel and the
    host's close (xlAutoClose only since dc-6332.10; earlier host traces also show
    xlAutoRemove first), so teardown calls are counted separately.
    """
    _, enters, leaves = trace
    close = min((x["seq"] for x in enters.values()
                 if x["entry"].endswith(("!xlAutoRemove", "!xlAutoClose"))), default=float("inf"))
    ids = {}
    window = event_token = None
    records = []
    identity_operands = {}
    previous_identity = {}
    for entry in enters.values():
        if not entry["entry"].endswith("!MdCallBack12"):
            continue
        end = leaves[entry["call"]]
        fn = entry["function"]
        args = normalize_x2(defined_boundary(entry["args"]))
        result = normalize_x2(defined_boundary(end.get("result")))
        raw_result = json.loads(json.dumps(result))
        identity = None
        if fn == 149 and end["return_code"] == 0:
            name = normalize_x2(entry["args"][3]["value"])
            require(result["type"] == 1 and result["value"] != 0, "Invalid registration ID")
            ids[result["value"]] = name
            result["value"] = "<registration:" + name + ">"
            identity = result["value"]
        elif fn == 16392 and end["return_code"] == 0:
            window = result["value"]
            require(window != 0, "Missing native window")
            result["value"] = "<window>"
            identity = result["value"]
        elif fn == 16401 and end["return_code"] == 0:
            event_token = result["value"]
            identity = "<event-registration-result>"
        elif fn == 16384:
            for index, arg in enumerate(args):
                pointers = entry.get("argument_addresses")
                source = (identity_operands.pop((entry["thread"], pointers[index]), None)
                          if pointers else previous_identity.get(entry["thread"]))
                if source and arg == source[0]:
                    arg["value"] = source[1]
            result = None
            if "args_after" in end:
                result = {"args_after": normalize_x2(defined_boundary(end["args_after"]))}
        elif fn == 201 and args and args[0].get("value") in ids:
            args[0]["value"] = "<registration:" + ids[args[0]["value"]] + ">"
        if fn != 16384:
            previous_identity[entry["thread"]] = (raw_result, identity) if identity else None
            pointer = entry.get("result_address")
            if pointer:
                identity_operands.pop((entry["thread"], pointer), None)
                if identity:
                    identity_operands[entry["thread"], pointer] = (raw_result, identity)
        else:
            previous_identity[entry["thread"]] = None
        if (entry["seq"] >= close) == teardown:
            records.append({"function": fn, "args": args, "return_code": end["return_code"], "result": result})
    return records


def framework_ownership(trace):
    _, enters, leaves = trace
    returned = [x for x in leaves.values() if ":" in x["entry"] and isinstance(x.get("result"), dict)
                and x["result"].get("type", 0) & 0x4000]
    frees = [x for x in enters.values() if x["entry"].endswith("!xlAutoFree12")]
    consumed = set()
    by_module = Counter()
    for value in sorted(returned, key=lambda x: x["seq"]):
        module = value["entry"].split("!", 1)[0]
        matches = [x for x in frees if x["call"] not in consumed and x["seq"] > value["seq"]
                   and x["oper_address"] == value["rax"] and x["thread"] == value["thread"]]
        require(matches, "Owned native result has no same-pointer/thread free")
        free = min(matches, key=lambda x: x["seq"])
        require(free["entry"].split("!", 1)[0] == module, "Wrong DLL owns xlAutoFree12")
        next_entry = min((x["seq"] for x in enters.values() if ":" in x["entry"] and x["seq"] > value["seq"]
                          and x["thread"] == value["thread"] and x["entry"].split("!", 1)[0] == module), default=float("inf"))
        require(free["seq"] < next_entry, "Owned result survived the next native module entry")
        consumed.add(free["call"])
        by_module[module] += 1
    require(len(consumed) == len(frees), "Unpaired native ownership callback")
    return {"owned_returns": len(returned), "free_calls": len(frees), "same_pointer_thread_and_module": True,
            "by_module": dict(by_module)}


def verify_xloil_reload(excel, excel_control, host, host_control, cases_path, publication_path=None):
    """Bounded host repair; Excel's original failed reload remains a failure."""
    directories = (excel, excel_control, host, host_control)
    envs = [json.loads((p / "environment.json").read_text(encoding="utf-8-sig")) for p in directories]
    cases = json.loads(cases_path.read_text(encoding="utf-8-sig"))
    expected = [{"formula": c["formula"], "value": c["value"]} for c in cases]
    require(len(expected) == 12, "Unqualified native reload cases")
    for index, env in enumerate(envs):
        require(env["fixture_files"] == envs[0]["fixture_files"], "Reload fixture files differ")
        require(env["cases_sha256"] == digest(cases_path), "Reload cases differ")
        if index >= 2:
            require(env["reload"], "Host reload was not requested")
    require(envs[0]["excel_file_version"] == envs[1]["excel_file_version"], "Excel control builds differ")
    require(envs[0]["tracer_sha256"] == envs[2]["tracer_sha256"], "Reload tracer binaries differ")
    require(envs[2]["executable_sha256"] == envs[3]["executable_sha256"] and envs[2]["shim_sha256"] == envs[3]["shim_sha256"], "Reload host binaries differ")
    excel_failures = []
    for path, env in zip(directories[:2], envs[:2]):
        require(env["pid"] != 3144 and env["pid"] not in env["preexisting_excel_pids"], "Unowned Excel PID")
        rows = jsonl(path / "controller.jsonl")
        require([r["value"] for r in rows if r["operation"] == "AutomationSecurity unchanged before teardown"] == [True], "Excel security state was not verified before failing teardown")
        values = [{"formula": r["operation"], "value": r["value"]} for r in rows if r["operation"].startswith("=")]
        require(values == expected, "Excel pre-reload formula outcomes differ")
        require(any(r["operation"] == "ExecuteExcel4Macro UNREGISTER(managed)" and r["value"] is True for r in rows), "Excel removal was not attempted")
        failures = [r for r in rows if r["operation"] == "failure"]
        require(len(failures) == 1 and "0x800706BE" in failures[0]["value"], "Original Excel reload failure changed")
        require(not any(r["operation"].startswith("reload =") for r in rows), "Excel unexpectedly returned reload values")
        require(any(r["operation"] == "cleanup" and r["value"] == "Owned Excel exited" for r in rows), "Failed Excel process remained live")
        excel_failures.append({"pid": env["pid"], "status": "failed reload, not a compatibility pass", "failure": failures[0]})
    raw = jsonl(excel / "boundary.jsonl")
    require([e["seq"] for e in raw] == list(range(1, len(raw) + 1)), "Failed Excel trace sequence is incomplete")
    require(any(e["event"] == "second_chance_exception" and e["code"] == "0xc0000005" for e in raw), "Original Excel access violation missing")
    require(raw[-1]["event"] == "process_exit" and raw[-1]["code"] != 0, "Original Excel did not exit on failure")
    excel_trace_errors = [e for e in raw if e["event"] in ("read_error", "debugger_error", "breakpoint_error", "unrecognized_breakpoint")]
    excel_enters = {e["call"]: e for e in raw if e["event"] == "enter"}
    opens = [e for e in excel_enters.values() if e["entry"] == "xlOil.xll!xlAutoOpen"]
    require(len(opens) == 2, "Excel second initialization was not entered")
    def phase(events, start, end):
        selected = [e for e in events if start <= e["seq"] < end]
        enters = {e["call"]: e for e in selected if e["event"] == "enter"}
        leaves = {e["call"]: e for e in selected if e["event"] == "leave"}
        require(enters.keys() == leaves.keys(), "Reload phase contains incomplete calls")
        for call, entry in enters.items():
            require(entry["entry"] == leaves[call]["entry"] and entry["thread"] == leaves[call]["thread"], "Reload phase call identity differs")
            inspect_values(entry)
            inspect_values(leaves[call])
        return selected, enters, leaves
    registrations = [e for e in excel_enters.values() if e["entry"] == "COM!Invoke" and e.get("dispid") == 30
                     and opens[0]["seq"] < e["seq"] < opens[1]["seq"] and len(e.get("rgvarg", [])) == 1
                     and e["rgvarg"][0].get("vt") == 8 and str(e["rgvarg"][0].get("value", "")).endswith("xlOil.xll")]
    require(len(registrations) == 1, "Failed Excel RegisterXLL call was not identified")
    first_excel = phase(raw, opens[0]["seq"], registrations[0]["seq"])
    require(not any(e["seq"] < opens[1]["seq"] for e in excel_trace_errors), "Debugger failure obscures Excel's pre-reload comparison")
    def native_records(trace):
        return [{"name": e["entry"].rsplit(":", 1)[1], **invocation_record(trace, e)} for e in trace[1].values() if ":" in e["entry"]]
    reference = native_records(first_excel)
    require(len(reference) == 13, "Original native invocation set changed")
    host_details = []
    for path in directories[2:]:
        status = json.loads((path / "host.exit.json").read_text(encoding="utf-8-sig"))
        require(status == {"exit_code": 0, "finished": True}, "Reload host failed or did not finish")
        require(not (path / "host.stderr").read_text(encoding="utf-8-sig").strip(), "Reload host reported diagnostics")
        output = json.loads((path / "host.stdout").read_text(encoding="utf-8-sig"))
        require(output["results"] == output["reload_results"] == expected, "Native host reload changed formula values")
        require(output["runtime_delay_releases"] == 1 and output["live_callback_allocations"] == 0, "Qualified runtime cleanup missing or repeated")
        require(output["closed_registration"] == "#NAME?", "Stale native callable was not rejected")
        host_details.append({"results": output["results"], "reload_results": output["reload_results"], "runtime_delay_releases": output["runtime_delay_releases"], "exit": status})
    trace = boundary(host)
    host_opens = [e for e in trace[1].values() if e["entry"] == "xlOil.xll!xlAutoOpen"]
    require(len(host_opens) == 2, "Host reload initialization missing or repeated")
    phases = [phase(trace[0], host_opens[0]["seq"], host_opens[1]["seq"]), phase(trace[0], host_opens[1]["seq"], trace[0][-1]["seq"])]
    records = [native_records(p) for p in phases]
    require(records[0] == records[1] == reference, "Ordered native calls/arguments/results/callbacks changed after reload")
    ownership = [framework_ownership(p) for p in [first_excel] + phases]
    require(ownership[0] == ownership[1] == ownership[2], "Native reload ownership differs")
    mapping = []
    for name in ("xlOil.xll", "xlOil.dll", "xlOil_SQL.dll", "xlOil_Utils.dll"):
        loads = [e for e in trace[0] if e["event"] == "module" and e.get("name") == name]
        unloads = [e for e in trace[0] if e["event"] == "module_unload" and e.get("name") == name]
        require(len(loads) == 2 and len(unloads) >= 1, "Native mapping cycle missing: " + name)
        require(loads[0]["seq"] < unloads[0]["seq"] < loads[1]["seq"], "Native image reused without unmapping: " + name)
        require(loads[0]["base"] == unloads[0]["base"] and loads[0]["path"] == unloads[0]["path"] == loads[1]["path"], "Native mapping identity differs: " + name)
        mapping.append({"name": name, "first_load": loads[0], "first_unload": unloads[0], "second_load": loads[1]})
    publications = verify_sources(envs, publication_path)
    return {"status": "verified bounded native host reload; original Excel reload remains failed", "formula_cases": len(cases),
            "native_invocations_each_phase": len(reference), "ordered_native_records": reference, "ownership_excel_host_first_host_second": ownership,
            "mapping_cycles": mapping, "host_controls": host_details, "excel_reload_failures": excel_failures, "failed_excel_register_entry": registrations[0], "failed_excel_trace_errors": excel_trace_errors,
            "new_host_object": [e["reload_new_host"] for e in envs[2:]], "source_bindings_checked": sum(len(e["source_files"]) + 2 for e in envs),
            "source_publication_revisions": [p["revision"] for p in publications],
            "scope": "Exact xlOil0.22.1 native Utils/SQL profile only. Python and overrides retain reload refusal, counted as failure. Renamed loaders remain unqualified/unsupported; a renamed first-load crash is retained separately. Arbitrary runtime teardown remains open."}


def verify_x2(excel, excel_control, host, host_control, cases_path, publication_path=None, x3=False, x4=False, native_entry_order=None):
    directories = (excel, excel_control, host, host_control)
    environments = [json.loads((p / "environment.json").read_text(encoding="utf-8-sig")) for p in directories]
    cases = json.loads(cases_path.read_text(encoding="utf-8-sig"))
    expected = [(c["formula"], c["value"]) for c in cases]
    def observed(value, env):
        value = normalize_x2(value)
        # Excel chooses blank-workbook names per process/session. Retain the raw
        # returned name and normalize only the independently recorded identity.
        if x4 and isinstance(value, str) and value.startswith(env["workbook_name"] + "|"):
            return "<workbook>" + value[len(env["workbook_name"]):]
        if x4 and isinstance(value, str) and value == env["workbook_name"]:
            return "<workbook>"
        if x4 and isinstance(value, str):
            for prefix in ("[" + env["workbook_name"] + "]", "'[" + env["workbook_name"] + "]"):
                if value.startswith(prefix):
                    return prefix.replace(env["workbook_name"], "<workbook>") + value[len(prefix):]
        if isinstance(value, dict):
            return {key: observed(child, env) for key, child in value.items()}
        if isinstance(value, list):
            return [observed(child, env) for child in value]
        return value
    root = Path(__file__).resolve().parents[2]
    if publication_path is None:
        candidate = excel.parent.parent / "source-publication.json"
        publication_path = candidate if candidate.exists() else None
    publications = verify_sources(environments, publication_path)
    for env in environments:
        require(env["fixture_files"] == environments[0]["fixture_files"], "Different managed fixture files")
        require(env["cases_sha256"] == digest(cases_path), "Case file differs from capture")
    require(environments[0]["excel_file_version"] == environments[1]["excel_file_version"], "Different Excel builds")
    require(environments[2]["executable_sha256"] == environments[3]["executable_sha256"], "Different host binaries")
    require(environments[2]["shim_sha256"] == environments[3]["shim_sha256"], "Different companion binaries")
    require(environments[0]["tracer_sha256"] == environments[2]["tracer_sha256"], "Different tracer binaries")
    require(environments[2]["executable_version"].startswith("16."), "Missing host version resource")
    outcomes = []
    host_outputs = []
    for directory, env in zip(directories[:2], environments[:2]):
        verify_excel_ownership(directory, env)
        observations = controller(directory)
        require(observations["AutomationSecurity unchanged"] is True, "Security setting changed")
        rows = [json.loads(line) for line in (directory / "controller.jsonl").read_text(encoding="utf-8-sig").splitlines() if line.strip()]
        outcomes.append([(r["operation"], observed(r["value"], env)) for r in rows if r["operation"].startswith("=")])
    for directory, env in zip(directories[2:], environments[2:]):
        if x3:
            status = json.loads((directory / "host.exit.json").read_text(encoding="utf-8-sig"))
            require(status == {"exit_code": 0, "finished": True}, "Host did not finish and exit successfully")
        output = json.loads((directory / "host.stdout").read_text(encoding="utf-8-sig"))
        require(not (directory / "host.stderr").read_text(encoding="utf-8-sig").strip(), "Host reported diagnostics")
        require(output["live_callback_allocations"] == 0, "Host callback storage leaked")
        require(output["closed_registration"] == "#NAME?" and output["closed_name"], "Old registration was not checked after close")
        require(not output["reload_results"] or output["reload_results"] == output["results"], "Reload changed formula outcomes")
        host_outputs.append(output)
        outcomes.append([(r["formula"], observed(r["value"], env)) for r in output["results"]])
    require(all([f for f, _ in actual] == [f for f, _ in expected] for actual in outcomes), "Missing or reordered formula cases")
    if environments[0]["fixture_name"] == "X0":
        command_values = {name: controller(excel)["Application.Run " + name] for name in ("X0.Queue", "X0.Context")}
        for output in host_outputs:
            for command in output["commands"]:
                require(command["value"] == command_values[command["command"]], "Native command return differs: " + command["command"])
    for index, (formula, value) in enumerate(expected):
        actual = [o[index][1] for o in outcomes]
        require(all(v == actual[0] for v in actual), "Formula differs between captures: " + formula)
        if value is not None:
            require(actual[0] == value, "Formula differs from expected value: " + formula)
        else:
            require(formula == "=X0.Caller()" and actual[0] == "ExcelReference sheet=<sheet> rows=2:2 cols=1:1", "Unchecked expected value")

    traces = [boundary(p) for p in (excel, host)]
    fixture_name = environments[0]["fixture_name"]
    measured_names = (["X0.Hello", "X0.Reference", "X0.Caller", "X0.Async", "X0.Big", "X0.ThreadSafe", "X0.NativeOwned"]
                      if fixture_name == "X0" else ["SayHello"])
    if x3:
        names = [{x["entry"].rsplit(":", 1)[1] for x in trace[1].values() if ":" in x["entry"]} for trace in traces]
        require(names[0] == names[1] and names[0], "Native function invocation names differ")
        registered = {normalize_x2(x["args"][3]["value"]) for x in traces[0][1].values()
                      if x.get("function") == 149}
        required = {name for case in cases for name in re.findall(r"([A-Za-z_][\w.]*)\s*\(", case["formula"])} & registered
        require(required <= names[0], "Formula calls lack native boundary observations: " + str(required - names[0]))
        measured_names = sorted(names[0])
    call_comparisons = {}
    opaque_storage = []
    initialization_context_commands = [[], []]
    def startup_context(trace, entry):
        if not x4 or not entry["entry"].endswith(":xloRunInXLLContext"):
            return False
        first_change = min(e["seq"] for e in trace[1].values()
                           if e["entry"] == "COM!Invoke" and e.get("dispid") == 0x61c
                           and e.get("object_label", "").startswith("AppEvents sink"))
        if entry["seq"] >= first_change:
            return False
        record = invocation_record(trace, entry)
        registrations = [c[1][3]["value"] for c in record["callbacks"] if c[0] == 149]
        if not registrations:
            return False
        require(Counter(registrations) == Counter(["xloAttr", "xloAttrObj", "xloImport", "xloPyDebug"]),
                "Startup context command registered an unexpected function set")
        require(all(c[0] in (149, 16384) and c[2] == 0 for c in record["callbacks"]), "Unexpected startup command callback")
        require(record["arguments"] == [] and record["result"] == {"type": 2048, "value": 1}, "Unexpected startup command native boundary")
        return True
    def measured_records(trace, name, index, defined=True):
        entries = [entry for entry in (native_entry_order[index] if native_entry_order is not None else trace[1].values()) if entry["entry"].endswith(":" + name)]
        rows = [invocation_record(trace, entry, defined) for entry in entries]
        if x4 and name == "xloRunInXLLContext":
            # Retain built-in registration before the first worksheet edit
            # separately from the event-driven command under comparison.
            startup = [row for entry, row in zip(entries, rows) if startup_context(trace, entry)]
            if defined:
                initialization_context_commands[index] = startup
            rows = [row for entry, row in zip(entries, rows) if not startup_context(trace, entry)]
        return rows
    for name in measured_names:
        records = [(measured_records(trace, name, index) if x3 else x2_invocation(trace, name)) for index, trace in enumerate(traces)]
        raw_records = [(measured_records(trace, name, index, False) if x3 else invocation_record(trace, next(e for e in trace[1].values() if e["entry"].endswith(":" + name)), False)) for index, trace in enumerate(traces)]
        if x4:
            records = [observed(rows, env) for rows, env in zip(records, (environments[0], environments[2]))]
            raw_records = [observed(rows, env) for rows, env in zip(raw_records, (environments[0], environments[2]))]
        raw_equal = (all_invocations(traces[0], name, False) == all_invocations(traces[1], name, False)
                     if native_entry_order is not None else raw_records[0] == raw_records[1])
        if name == "X0.Async":
            # Excel-DNA's ObjectParam reads hData only for XlTypeBigData
            # (XlMarshalContext.cs:597-600). Keep cbData differences explicit;
            # they are not evidence that every raw argument byte agrees.
            for record, raw_record in zip(records, raw_records):
                handle = record["arguments"][1]
                require(handle["code"] == "X" and handle["value"]["type"] == 2050, "Async handle type differs")
                opaque_storage.append(handle["value"]["value"]["byte_count"])
                if "abi_storage" not in raw_record["arguments"][1]["value"]:
                    handle["value"]["value"].pop("byte_count")
        require(records[0] == records[1], "Defined native arguments/result/nested callbacks differ for " + name)
        call_comparisons[name] = {"same_defined_arguments_and_result": True, "same_raw_record_after_identity_normalization": raw_equal,
                                  "invocations_each_host": len(records[0]) if x3 else 1,
                                  "same_ordered_callbacks": [r["callbacks"] for r in records[0]] if x3 else records[0]["callbacks"]}
        if name == "X0.Async":
            call_comparisons[name]["opaque_handle_cbData_excel_and_host"] = opaque_storage

    signatures = []
    callback_counts = []
    boundary_summary = []
    for directory, trace in zip((excel, host), traces):
        events, enters, leaves = trace
        first = next(x for x in enters.values() if x["entry"].endswith(":" + measured_names[0]))["seq"]
        registrations = {}
        for x in enters.values():
            if x["entry"].endswith("!MdCallBack12") and x["function"] == 149 and x["seq"] < first:
                require(leaves[x["call"]]["return_code"] == 0, "Registration failed")
                args = x["args"]
                registrations[normalize_x2(args[3]["value"])] = {"procedure": args[1]["value"], "type": args[2]["value"], "macro_type": args[5]["value"] if len(args) > 5 else 1}
        signatures.append(registrations)
        exports = [e for e in events if e["event"] == "xll_exports" and e["module"] == fixture_name + "-AddIn64.xll"]
        if not x3:
            require(exports and exports[0]["numbered_stub_count"] == 10000, "Original Excel-DNA export pool not observed")
        versions = [x for x in enters.values() if x["entry"].endswith("!XLCallVer")]
        require((versions or fixture_name == "X2Aot" or x3) and all(int(leaves[x["call"]]["rax"], 16) & 0xffffffff == 3072 for x in versions), "XLCallVer differs")
        codes = Counter((x["function"], x["return_code"]) for x in leaves.values() if "return_code" in x)
        callback_counts.append(codes)
        if fixture_name == "X0":
            for fn in (16393, 16385, 89, 16384):
                require(codes[fn, 32] >= 1, "Missing outside-context failure: " + str(fn))
            rows = fixture(directory)
            main = int(next(x[1] for x in rows if x[2] == "AutoOpen"))
            require(all(int(x[1]) == main for x in rows if x[2] in ("Hello", "Reference argument", "Caller", "Queue executed", "QueueAsMacro enter")), "Main-lane case used another thread")
            workers = [x for x in rows if x[2] == "ThreadSafe"]
            require(workers and all(int(x[1]) != main for x in workers), "Thread-safe case did not use a worker")
            require(any(x[2] == "Async SetResult" and x[3] == "True" and int(x[1]) != main for x in rows), "Async completion missing")
            require(any(x[2] == "Queue executed" and x[3].endswith("X0-AddIn64.xll") for x in rows), "Queued macro lost originating XLL")
            pen = [x for x in enters.values() if x["entry"].endswith("!LPenHelper")]
            require(pen and all(leaves[x["call"]]["output_first_long"] == 0 and int(leaves[x["call"]]["rax"], 16) & 0xffffffff == 0 for x in pen), "Ready-mode check failed")
            run = [x for x in enters.values() if x["entry"] == "COM!Invoke" and x.get("dispid") == 259 and any("SyncMacro_" in str(a.get("value", "")) for a in x["rgvarg"])]
            require(run and all(leaves[x["call"]]["result"] == {"vt": 5, "value": 0} for x in run), "SyncMacro COM return differs")
            require(any(x["entry"].endswith("!AccessibleObjectFromWindow") for x in enters.values()), "Application lookup not observed")
            owned = [x for x in leaves.values() if x["entry"].endswith(":X0.NativeOwned")]
            frees = [x for x in enters.values() if x["entry"] == "NativeOwner64.xll!xlAutoFree12"]
            require(len(owned) == len(frees) > 0, "Ownership case missing")
            for returned in owned:
                matches = [x for x in frees if returned["seq"] < x["seq"] and returned["rax"] == x["oper_address"] and returned["thread"] == x["thread"]]
                require(matches and returned["result"]["type"] == 0x4002, "DLLFree pointer/thread mismatch")
            require(any(x["result"]["type"] == 64 for x in leaves.values() if x["entry"].endswith(":X0.Big")), "Original mixed array not observed")
        boundary_summary.append({"records": len(events), "paired_calls": len(enters), "c_api_calls": sum(codes.values()), "trace_sha256": digest(directory / "boundary.jsonl")})
    require(signatures[0] == signatures[1], "Startup registration forms differ")
    if fixture_name == "X0":
        for output in host_outputs:
            require(output["reload_results"], "Managed runtime reload was not exercised")
            require(any(c.get("early_bound") and c["value"] == command_values["X0.Queue"] for c in output["commands"]), "Early-bound Run was not exercised")
        # The same original net48 X0 capture remains the callback-number/code reference.
        old = boundary(Path(__file__).resolve().parent / "traces/2026-11-01-net48/traced")
        old_codes = {(x["function"], x["return_code"]) for x in old[2].values() if "return_code" in x}
        current_codes = set(callback_counts[1])
        require(old_codes <= current_codes, "An X0 callback/code pair is absent from the host")
    else:
        old_codes = set()
    x3_details = {}
    async_callback_differences = []
    if x3:
        callbacks = [framework_callbacks(trace) for trace in traces]
        require([(r["function"], r["return_code"]) for r in callbacks[0]] ==
                [(r["function"], r["return_code"]) for r in callbacks[1]], "Pre-close callback sequence/codes differ")
        differences = [{"index": i, "excel": a, "host": b} for i, (a, b) in enumerate(zip(*callbacks)) if a != b]
        registrations = [Counter(json.dumps(r, sort_keys=True) for r in rows if r["function"] == 149) for rows in callbacks]
        require(registrations[0] == registrations[1], "A registration differs beyond its order/opaque identity")
        # These differences are reported, not counted as raw boundary matches.
        for difference in differences:
            a, b = difference["excel"], difference["host"]
            if a["function"] == 16400:
                require(a["args"] == b["args"] and a["return_code"] == b["return_code"] == 0
                        and a["result"] == {"type": 0, "value": {"raw32": "00" * 32}}
                        and b["result"] == {"type": 4, "value": True}, "Unqualified native async callback difference")
                async_callback_differences.append(difference)
            else:
                require(a["function"] in (149, 614, 16401, 16384), "Unexplained callback difference")
        ownership = [framework_ownership(trace) for trace in traces]
        require(ownership[0] == ownership[1], "Native ownership counts/modules differ")
        dynamic = [[x for x in trace[0] if x["event"] == "registered_entry" and "!xloil_stub:" in x["entry"]] for trace in traces]
        x3_details = {"pre_close_callbacks_each_host": len(callbacks[0]), "same_ordered_callback_numbers_and_codes": True,
                      "callbacks_equal_after_identity_normalization": len(callbacks[0]) - len(differences),
                      "raw_callback_differences": differences, "ownership_excel_and_host": ownership,
                      "rewritten_export_registrations_each_host": [len(x) for x in dynamic],
                      "teardown_callbacks_excel_and_host": [framework_callbacks(trace, teardown=True) for trace in traces],
                      "teardown_scope": "Excel UNREGISTER(path); the host's close enters xlAutoClose only since dc-6332.10 (retained earlier host traces show xlAutoRemove first). Teardown actions stay separately counted."}
    entry_cases = [case for case in cases if "entry_wait_value" in case]
    if entry_cases:
        for directory in (excel, excel_control):
            observed_entry = controller(directory)
            for case in entry_cases:
                require(observed_entry["formula-entry " + case["formula"]] == case["entry_wait_value"], "Excel formula-entry completion differs")
                if "before_calculate" in case:
                    require(observed_entry["before Calculate " + case["before_calculate"]] == case["before_calculate_value"], "Excel entry-time invocation count differs")
        for output in host_outputs:
            for case in entry_cases:
                entry = [row for row in output["formula_entries"] if row["cell"] == "B3" and row["formula"] == case["formula"]]
                require(len(entry) == 1 and entry[0]["value"] == case["entry_wait_value"] and "reason: FormulaEntry" in entry[0]["provenance"]["stamp"], "Host has no actual producing formula-entry completion")
                require(entry[0]["provenance"]["cooperative_invocations"] == entry[0]["provenance"]["completed_invocations"] == 1, "Formula entry did not resume its producing invocation")
                require(len(output["async_returns"]) == case["expected_async_returns"] and all(r["disposition"] == "Accepted" for r in output["async_returns"]), "Native entry/calculation P07 completions differ")
                require(not output["rtd_updates"], "Native async formula entry used RTD")
        x3_details["native_async_formula_entry"] = {"cases": len(entry_cases), "p07_returns_each_host_control": len(host_outputs[0]["async_returns"]), "raw_callback_result_differences": async_callback_differences,
            "scope": "Actual owner formula-entry completion and later Manual Calculate. Raw xlAsyncReturn result operands are compared; every difference remains a failed strict comparison."}
    x4_details = {}
    if x4:
        def com_calls(trace, start, end, thread, env):
            records = []
            for entry in trace[1].values():
                if entry["thread"] != thread or not (start < entry["seq"] < trace[2][entry["call"]]["seq"] < end):
                    continue
                if entry["entry"].startswith("COM!early."):
                    returned = trace[2][entry["call"]]
                    record = {"entry": entry["entry"], "hresult": returned["hresult"]}
                    if "value" in returned: record["value"] = observed(returned["value"], env)
                    if "object" in returned: record["returned_object"] = bool(int(returned["object"], 16))
                    if "index" in entry: record["index"] = observed(entry["index"], env)
                    if "arguments" in entry: record["arguments"] = observed(entry["arguments"], env)
                    if "result" in returned: record["result"] = observed(returned["result"], env)
                    records.append(record)
            return records
        nested = []
        changes = []
        delivered = []
        expected_event_cells = ["A1"]
        events_enabled = True
        for case in cases:
            events_enabled = case.get("enable_events", events_enabled)
            if events_enabled:
                if "edit" in case:
                    expected_event_cells.append(case["edit"]["cell"])
                expected_event_cells.append("B3")
        for trace, env in zip(traces, (environments[0], environments[2])):
            by_udf = {}
            for entry in trace[1].values():
                if ":" in entry["entry"]:
                    name = entry["entry"].rsplit(":", 1)[1]
                    if startup_context(trace, entry):
                        continue
                    by_udf.setdefault(name, []).append(com_calls(trace, entry["seq"], trace[2][entry["call"]]["seq"], entry["thread"], env))
            nested.append(by_udf)
            sheet_changes = []
            event_counts = Counter()
            for entry in trace[1].values():
                if entry["entry"] != "COM!Invoke" or not entry.get("object_label", "").startswith("AppEvents sink"):
                    continue
                event_counts[entry["dispid"]] += 1
                if entry["dispid"] != 0x61c: continue
                require(entry["flags"] == 3 and entry["named_dispids"] == [0, 1], "SheetChange flags/named arguments differ")
                require([v["vt"] for v in entry["rgvarg"]] == [9, 9], "SheetChange needs worksheet then Range dispatch objects")
                require(trace[2][entry["call"]]["hresult"] == 0, "SheetChange sink failed")
                end = trace[2][entry["call"]]["seq"]
                addresses = [trace[2][call["call"]]["result"] for call in trace[1].values()
                             if call["entry"] == "COM!Invoke" and call.get("dispid") == 0xec
                             and call["thread"] == entry["thread"]
                             and entry["seq"] < call["seq"] < trace[2][call["call"]]["seq"] < end]
                sheet_changes.append({"flags": entry["flags"], "named_dispids": entry["named_dispids"],
                                      "argument_types": [v["vt"] for v in entry["rgvarg"]], "addresses": observed(addresses, env),
                                      "early_bound_calls": com_calls(trace, entry["seq"], end, entry["thread"], env)})
            changes.append(sheet_changes)
            delivered.append(dict(sorted(event_counts.items())))
            require(len(sheet_changes) == len(expected_event_cells), "Missing or additional SheetChange deliveries")
            for change, cell in zip(sheet_changes, expected_event_cells):
                names = [r for r in change["early_bound_calls"] if r["entry"] == "COM!early.worksheet.Name"]
                require(names == [{"entry": "COM!early.worksheet.Name", "hresult": 0, "value": "X0"}], "Worksheet.Name was not observed once in every SheetChange")
                require(change["addresses"] == [{"vt": 8, "value": "[<workbook>]X0!" + cell}, {"vt": 8, "value": cell}], "Both Range.Address calls were not observed in every SheetChange")
            unadvise = [entry for entry in trace[1].values() if entry["entry"] == "COM!Unadvise"]
            require(unadvise and all(trace[2][e["call"]]["hresult"] == 0 for e in unadvise), "Event disconnection was not successful")
        require(nested[0] == nested[1], "Early-bound calls inside a measured UDF differ")
        require(changes[0] == changes[1] and changes[0], "Ordered SheetChange calls/addresses differ")
        a1 = sum(any(a == {"vt": 8, "value": "A1"} for a in change["addresses"]) for change in changes[0])
        require(a1 == 3, "A1 changes were missing or delivered while EnableEvents was false")
        require(nested[0].get("xloRunInXLLContext"), "The xlOil context command was not observed")
        require(sum(call["entry"] == "COM!early.application.Run" for change in changes[0] for call in change["early_bound_calls"]) == 1,
                "The single event-driven early-bound Application.Run was not measured")
        require(any(nested[0].get("X4.PyCom", [])), "No early-bound application/workbook/worksheet reads")
        x4_details = {"nested_early_bound_com_calls": nested[0], "ordered_sheet_changes": changes[0],
                      "initialization_context_commands_excel_and_host": initialization_context_commands,
                      "sheet_change_count_each_host": len(changes[0]), "a1_changes_each_host": a1,
                      "all_delivered_event_counts_excel_and_host": delivered,
                      "event_scope": "Ordered SheetChange calls and addresses agree; other event counts are retained separately. Retained Python COM objects/futures are explicitly released before teardown.",
                      "raw_workbook_names": [env["workbook_name"] for env in environments]}
    return {"status": "failed strict async callback result comparison" if async_callback_differences else "verified X4 measured cases" if x4 else "verified X3 measured cases" if x3 else "verified X2 measured cases", "fixture": fixture_name,
            "formula_cases": len(cases), "matched_each_capture": len(cases), "controls_match": True,
            "source_revisions": sorted({e["repository_commit"] for e in environments}),
            "source_publication_revisions": [p["revision"] for p in publications],
            "same_fixture_files": environments[0]["fixture_files"], "registration_forms": signatures[0],
            "native_invocations": call_comparisons, "excel_and_host": boundary_summary,
            "x0_callback_code_pairs_observed": len(old_codes),
            "callback_counts_excel_and_host": [[{"function": fn, "return_code": rc, "count": count} for (fn, rc), count in sorted(c.items())] for c in callback_counts],
            "reload_checked": bool(host_outputs[0]["reload_results"]),
            "identity_normalization": "Opaque sheet/async identities, per-add-in GUID names and absolute XLL directories; X4 also replaces only the independently recorded blank-workbook name. xlFree has no defined result operand; raw differences remain in the trace and native raw-equality flags. Early COM object results check nonnull presence, not a complete identity graph. Other value/type/ownership bits and nested callback order remain exact.",
            "scope": ("Original xlOil Python; measured single-workbook properties, local single-cell event ranges, SheetChange delivery and context command. General COM, RTD, arbitrary Python teardown and safe reload remain open."
                      if x4 else "Single-invocation worker dispatch; no simultaneous workbook calculation, RTD, cancellation, delivered host AppEvents or full COM model claim."
                      if fixture_name == "X0" else "All observed native UDF invocations, including repetitions; no full COM, event delivery, RTD or reload claim." if x3 else "Hello only; no COM, worker, async or reload claim."),
            "known_difference": ("xlAsyncReturn raw result operand: Excel type zero, host documented Boolean. Raw opaque handle cbData is retained separately; Excel-DNA reads only hData on entry. Codes and published values agree."
                                 if fixture_name == "X0" else None),
            **x3_details,
            **x4_details,
            "timing": "Debugger timing is not uninstrumented performance evidence."}


def verify_rtd_extended(excel, excel_control, host, host_control, cases_path, publication_path):
    """Retain every call in broader RTD runs; scheduling differences stay failures."""
    directories = (excel, excel_control, host, host_control)
    envs = [json.loads((p / "environment.json").read_text(encoding="utf-8-sig")) for p in directories]
    cases = json.loads(cases_path.read_text(encoding="utf-8-sig"))
    require(all(e["fixture_files"] == envs[0]["fixture_files"] and e["cases_sha256"] == digest(cases_path) for e in envs), "Different RTD fixtures/cases")
    require(envs[0]["excel_file_version"] == envs[1]["excel_file_version"] and envs[0]["tracer_sha256"] == envs[2]["tracer_sha256"], "Different RTD Excel builds/tracers")
    require(envs[2]["executable_sha256"] == envs[3]["executable_sha256"] and envs[2]["shim_sha256"] == envs[3]["shim_sha256"], "Different RTD host binaries")
    publications = verify_sources(envs, publication_path)
    names = {c["formula"] for c in cases}
    for directory, env in zip(directories[:2], envs[:2]):
        control = controller(directory)
        verify_excel_ownership(directory, env)
        require(control["AutomationSecurity unchanged"], "RTD security changed")
        require(control["Close RTD workbook before detach"], "RTD workbook stayed open at detach")
        events = jsonl(directory / "controller.jsonl")
        actual = [(r["operation"], r["value"]) for r in events if r["operation"] in names]
        require(actual == [(c["formula"], c["value"]) for c in cases], "Ordered Excel RTD values differ")
        dependent = [r["value"] for r in events if r["operation"].startswith("dependent =")]
        require(dependent == [c["dependent_value"] for c in cases if "dependent" in c], "Ordered Excel RTD dependents differ")
        for case in cases:
            if "rtd_idle_ms" in case:
                require(control["RTD idle begin " + case["formula"]] == case["rtd_idle_ms"]
                        and control["RTD idle complete " + case["formula"]] >= case["rtd_idle_ms"], "Excel RTD idle observation was shortened")
            if case.get("rtd_variant_array"):
                require(control["raw " + case["formula"]] == -2146826246, "Excel array RTD value is not a raw #N/A error")
    outputs = []
    for directory in directories[2:]:
        require(json.loads((directory / "host.exit.json").read_text()) == {"exit_code": 0, "finished": True}, "RTD host failed")
        require(not (directory / "host.stderr").read_text().strip(), "RTD host diagnostics")
        output = json.loads((directory / "host.stdout").read_text())
        require(output["live_callback_allocations"] == output["rtd_subscriptions_after_close"] == 0 and output["closed_registration"] == "#NAME?", "RTD teardown incomplete")
        # OneCalc has one formula and no dependents (output["single_formula"]): its rows carry none.
        if output.get("single_formula"):
            require([(r["formula"], r["value"]) for r in output["results"]] == [(c["formula"], c["value"]) for c in cases], "Ordered host RTD outcomes differ")
        else:
            require([(r["formula"], r["value"], r["dependent"]) for r in output["results"]] == [(c["formula"], c["value"], c.get("dependent_value")) for c in cases], "Ordered host RTD outcomes differ")
        latest = {}
        for update in output["rtd_updates"]:
            key = ident(update)
            require(update["disposition"] == "Accepted" and update["sequence"] == latest.get(key, 0) + 1, "RTD input was rejected or its sequence skipped/rewound")
            latest[key] = update["sequence"]
        for case, row in zip(cases,output["results"],strict=True):
            if "rtd_idle_ms" in case:
                require(row["idle_observed_ms"] >= case["rtd_idle_ms"], "Host RTD idle observation was shortened")
            if "expected_rtd_inputs" in case:
                require(len(row["provenance"]["rtd_inputs"]) == case["expected_rtd_inputs"] and row["provenance"]["completed_invocations"] == case["expected_completed_invocations"], "RTD value lacks its expected producing invocations/inputs")
                if not output.get("single_formula"):
                    require(len(row["dependent_provenance"]["rtd_inputs"]) == case["expected_dependent_rtd_inputs"] and row["dependent_provenance"]["completed_invocations"] == case["expected_dependent_completed_invocations"], "RTD dependent lacks its expected producing invocations/inputs")
            through = row["rtd_updates_through"]
            require(isinstance(through, int) and 0 <= through <= len(output["rtd_updates"]), "Invalid RTD producing cut index")
            cut = {ident(u): u["sequence"] for u in output["rtd_updates"][:through]}
            for proof in (row["provenance"], row.get("dependent_provenance")):
                if proof:
                    for read in proof["rtd_inputs"]:
                        require(cut.get(ident(read)) == read["sequence"], "RTD certificate does not name its latest admitted cut")
        require(all(r["disposition"] == "Accepted" for r in output["rtd_retired"]), "RTD retirement failed")
        require(not output["async_returns"] and not output["revoked_tokens"], "RTD used native async completion")
        if any(c.get("keep_formula") for c in cases):
            require([(ident(u)[1], u["sequence"], u["value"]) for u in output["rtd_updates"]] == [(1, 1, 1), (1, 2, 42), (1, 3, 1), (1, 4, 42)], "Completed-topic reuse did not preserve ID and advance the input sequence")
            require([len(r["provenance"]["rtd_inputs"]) for r in output["results"]] == [1, 0, 1, 0], "RTD branch/reuse certificates differ")
        if any(c.get("rtd_variant_array") for c in cases):
            require(len(cases) == 1, "Unqualified mixed array RTD profile")
            updates = output["rtd_updates"]
            require(len(updates) == 2
                    and len({ident(u) for u in updates}) == 1
                    and [(u["sequence"], u["value"], u["disposition"]) for u in updates]
                        == [(1, {"error":"#N/A"}, "Accepted"), (2, {"error":"#N/A"}, "Accepted")], "Array RTD lacks its initial and refreshed admitted #N/A cuts")
            last = updates[-1]
            require(output["results"][0]["rtd_updates_through"] == 2
                    and output["results"][0]["provenance"]["rtd_inputs"]
                        == [{**ident_fields(last), "sequence":2}], "Array RTD certificate does not select the refreshed input")
        outputs.append(output)
    protocols, native, internal, identities = [], [], [], []
    methods = {10: "ServerStart", 11: "ConnectData", 12: "RefreshData", 13: "DisconnectData", 14: "Heartbeat", 15: "ServerTerminate"}
    for index, directory in enumerate((excel, host)):
        trace = boundary(directory)
        _, enters, leaves = trace
        opening = next(e for e in enters.values() if e["entry"].endswith("!xlAutoOpen"))
        topics, servers, guid_topics = {}, {}, {}
        activations = []
        protocol, bodies = [], []
        for entry in enters.values():
            returned = leaves[entry["call"]]
            dispatch = entry["entry"] == "COM!Invoke" and entry.get("object_label") == "RTD server"
            early = entry["entry"].startswith("COM!early.rtdserver.")
            if entry["entry"] == "COM!early.rtdsink.UpdateNotify":
                protocol.append({"method": "UpdateNotify", "hresult": returned["hresult"]})
                continue
            if not dispatch and not early:
                continue
            parent = [e for e in enters.values() if early and e["entry"] == "COM!Invoke" and e.get("object") == entry.get("object") and e["thread"] == entry["thread"] and e["seq"] < entry["seq"] < returned["seq"] < leaves[e["call"]]["seq"]]
            if parent:
                outer_entry = min(parent, key=lambda e: e["seq"])
                outer = leaves[outer_entry["call"]]
                method = entry["entry"].rsplit(".", 1)[-1]
                require(methods.get(outer_entry["dispid"]) == method and outer["hresult"] == returned["hresult"], "Forwarded RTD body changed method/HRESULT")
                if method in ("ServerStart", "Heartbeat"):
                    require(outer["result"]["value"] == returned["started" if method == "ServerStart" else "value"], "Forwarded RTD scalar differs")
                elif method == "ConnectData":
                    require(outer["result"] == returned["result"] and outer["arguments_after"][0]["value"]["value"] == returned["new_values"], "Forwarded RTD connection differs")
                elif method == "RefreshData":
                    require(outer["result"]["value"] == returned["data"] and outer["arguments_after"][0]["value"]["value"] == returned["count"], "Forwarded RTD refresh differs")
                bodies.append({"entry": entry, "returned": returned})
                continue
            if early and entry["entry"].endswith(".ServerTerminate") and entry.get("from", {}).get("module") in envs[0]["fixture_files"]:
                releases = [e for e in enters.values() if e["entry"] == "COM!Release" and e.get("object") == entry["object"] and e["thread"] == entry["thread"] and e["seq"] < entry["seq"] < returned["seq"] < leaves[e["call"]]["seq"]]
                require(len(releases) == 1 and leaves[releases[0]["call"]].get("references") == 0, "RTD destructor lacks its final same-object Release")
                require(releases[0].get("from", {}).get("module") in (("EXCEL.EXE",) if index == 0 else HOST_MODULES), "RTD destructor has another owner")
                require(protocol and protocol[-1] == {"method":"ServerTerminate", "server":servers[entry["object"]], "hresult":0} and returned["hresult"] == 0, "RTD destructor lacks prior successful termination")
                bodies.append({"entry":entry,"returned":returned,"reason":"final Release destructor","release":releases[0],"release_returned":leaves[releases[0]["call"]]})
                continue
            require(entry["thread"] == opening["thread"], "RTD client call left the owner STA")
            require(entry.get("from", {}).get("module") in (("EXCEL.EXE",) if index == 0 else HOST_MODULES), "RTD external client has an unexpected caller")
            method = methods[entry["dispid"]] if dispatch else entry["entry"].rsplit(".", 1)[-1]
            obj = entry["object"]
            if method == "ServerStart":
                created = [e for e in enters.values() if (e["entry"] == "COM!CreateInstance" or e["entry"].endswith("!CoCreateInstance")) and e.get("iid") == "ec0e6191-db51-11d3-8f3e-00c04f3651b8" and leaves[e["call"]].get("object") == obj and leaves[e["call"]]["seq"] < entry["seq"]]
                require(created, "RTD server lacks a recorded interface activation")
                servers[obj] = len(activations)
                activations.append({"object":obj,"creation":created[-1],"started_by":entry})
            require(obj in servers, "RTD call preceded ServerStart")
            row = {"method": method, "server": servers[obj], "hresult": returned["hresult"]}
            args, after = entry.get("rgvarg", []), returned.get("arguments_after", [])
            if method == "ServerStart": row["started"] = returned["result"]["value"] if dispatch else returned["started"]
            elif method == "ConnectData":
                topic = args[2]["value"] if dispatch else entry["topic"]
                topics.setdefault(topic, len(topics))
                strings = json.loads(json.dumps(args[1]["value"] if dispatch else entry["strings"]))
                for value in strings["values"]:
                    if value.get("vt") == 8 and re.fullmatch(r"[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}", value.get("value", "")):
                        text = value["value"]; guid_topics.setdefault(text, len(guid_topics)); value["value"] = "<async-topic-%d>" % guid_topics[text]
                row.update(topic=topics[topic], strings=strings, new_before=args[0]["value"]["value"] if dispatch else entry["new_values"], new_after=after[0]["value"]["value"] if dispatch else returned["new_values"], initial=returned["result"])
            elif method == "RefreshData":
                count = after[0]["value"]["value"] if dispatch else returned["count"]
                data = json.loads(json.dumps(returned["result"]["value"] if dispatch else returned["data"]))
                require(count >= 0 and (data is not None or count == 0), "Missing RTD refresh array")
                if data is not None:
                    require(data["dimensions"] == 2 and not data["truncated"] and len(data["values"]) == 2 * count, "Incomplete RTD refresh array")
                for i in range(count):
                    item = data["values"][2*i]; require(item["vt"] == 3 and item["value"] in topics, "RTD refresh has an unknown topic"); item["value"] = topics[item["value"]]
                row.update(count=count, data=data)
            elif method == "DisconnectData":
                topic = args[0]["value"] if dispatch else entry["topic"]
                require(topic in topics, "RTD disconnected an unknown topic"); row["topic"] = topics[topic]
            elif method == "Heartbeat": row["value"] = returned["result"]["value"] if dispatch else returned["value"]
            protocol.append(row)
        require(all(r["hresult"] == 0 for r in protocol), "RTD protocol call failed")
        protocols.append(protocol); internal.append(bodies); identities.append({"servers": activations, "topics": topics, "async_topics": guid_topics})
        native.append([{"name": e["entry"].rsplit(":", 1)[1], **invocation_record(trace, e)} for e in enters.values() if ":" in e["entry"]])
    require(all(protocols) and all(native), "RTD comparison has no observed client or native calls")
    if any(c.get("keep_formula") or c.get("rtd_variant_array") for c in cases):
        required_methods = {"ServerStart", "ConnectData", "UpdateNotify", "RefreshData", "DisconnectData", "ServerTerminate"}
        require(all(required_methods <= {r["method"] for r in protocol} for protocol in protocols), "RTD array/reuse lifecycle coverage is incomplete")
    if any(c.get("rtd_variant_array") for c in cases):
        def array_value(values):
            return {"vt":8204, "value":{"dimensions":2, "element_bytes":24,
                "bounds":[{"count":2,"lower":0},{"count":2,"lower":0}],
                "values":[{"vt":5,"value":v} for v in values], "truncated":False}}
        for protocol, rows in zip(protocols, native, strict=True):
            initial = [r["initial"] for r in protocol if r["method"] == "ConnectData"]
            refreshed = [r["data"]["values"][2*i+1]
                         for r in protocol if r["method"] == "RefreshData" and r["data"] is not None
                         for i in range(r["count"])]
            require(initial == [array_value([1,3,2,4])], "Array RTD ConnectData did not return the complete VARIANT array")
            require(refreshed == [array_value([42,44,43,45])], "Array RTD RefreshData did not return the complete updated VARIANT array")
            callbacks = [c for row in rows for c in row["callbacks"] if c[0] == 379]
            require(callbacks and all(c[2] == 0 and c[3] == {"type":16,"value":42} for c in callbacks), "Array RTD C API callback did not return #N/A")
    pairs = [{"position": i, "excel": a, "host": b, "matched": a == b} for i, (a, b) in enumerate(zip_longest(*protocols))]
    normalized_native = json.loads(json.dumps(native))
    for rows, identity, env in zip(normalized_native, identities, (envs[0],envs[2])):
        for row in rows:
            for callback in row["callbacks"]:
                if callback[0] != 379: continue
                args = callback[1]
                if re.fullmatch(r"RtdSrv_[a-f0-9]{32}", str(args[0].get("value", ""))) or args[0].get("value")==f"DnaCalc.X5.{env['pid']}":
                    args[0]["value"] = "<private-rtd-progid>"
                for value in args[2:]:
                    if value.get("value") in identity["async_topics"]:
                        value["value"] = "<async-topic-%d>" % identity["async_topics"][value["value"]]
    native_pairs = [{"position": i, "excel": a, "host": b, "matched": a == b} for i, (a,b) in enumerate(zip_longest(*normalized_native))]
    strict = all(p["matched"] for p in pairs + native_pairs)
    result = {"status": "verified RTD values and ordered client/native calls" if strict else "failed strict RTD client/native comparison", "formula_cases": len(cases), "matched_values_each_capture": len(cases), "controls_match": True, "source_bindings_checked": sum(len(e["source_files"]) + 2 for e in envs), "source_publication_revisions": [p["revision"] for p in publications], "client_protocol_comparison": pairs, "client_calls_matched": sum(p["matched"] for p in pairs), "client_calls_failed": sum(not p["matched"] for p in pairs), "native_invocation_comparison": native_pairs, "native_calls_matched": sum(p["matched"] for p in native_pairs), "native_calls_failed": sum(not p["matched"] for p in native_pairs), "raw_native_invocations_excel_and_host": native, "internal_server_bodies": internal, "identity_bindings_excel_and_host": identities, "host_results": outputs, "scope": "Every value, dependent and P07 certificate checked. Every observed client and native call retained; unequal ordered client traffic stays a failure. RTD input is distinct from native async completion. Topic IDs and GUID topic strings are normalized only by first observed identity; raw bindings remain."}
    return result


def verify_rtd(excel, excel_control, host, host_control, cases_path, publication_path):
    cases = json.loads(cases_path.read_text(encoding="utf-8-sig"))
    if any(c.get("keep_formula") or c.get("rtd_variant_array") or "expected_rtd_inputs" in c for c in cases) or any("MyAsync" in c["formula"] for c in cases):
        return verify_rtd_extended(excel, excel_control, host, host_control, cases_path, publication_path)
    directories = (excel, excel_control, host, host_control)
    environments = [json.loads((p / "environment.json").read_text(encoding="utf-8-sig")) for p in directories]
    cases = json.loads(cases_path.read_text(encoding="utf-8-sig"))
    signaling = any("X5.Rtd()" in c["formula"] for c in cases)
    outputs = []
    for env in environments:
        require(env["fixture_files"] == environments[0]["fixture_files"], "Different RTD fixture binaries")
        require(env["cases_sha256"] == digest(cases_path), "RTD cases differ")
    require(environments[2]["executable_sha256"] == environments[3]["executable_sha256"], "Different RTD host executables")
    require(environments[0]["excel_file_version"] == environments[1]["excel_file_version"], "Different RTD Excel builds")
    require(environments[2]["shim_sha256"] == environments[3]["shim_sha256"], "Different RTD companion binaries")
    require(environments[0]["tracer_sha256"] == environments[2]["tracer_sha256"], "Different RTD tracers")
    for directory, env in zip(directories[:2], environments[:2]):
        observed = controller(directory)
        if excel_session_directory is not None:
            verify_excel_ownership(directory, env)
        else:
            require(env["pid"] != 3144 and env["pid"] not in env["preexisting_excel_pids"], "Unowned RTD Excel PID")
        require(observed["AutomationSecurity unchanged"] and observed["RTD termination before detach"], "RTD cleanup/security control failed")
        for case in cases:
            require(observed[case["formula"]] == case["value"], "Excel RTD formula differs")
            require(observed["dependent " + case["formula"]] == case["dependent_value"], "Excel RTD dependent differs")
            if "rtd_idle_ms" in case:
                require(observed["RTD idle begin " + case["formula"]] == case["rtd_idle_ms"] and observed["RTD idle complete " + case["formula"]] >= case["rtd_idle_ms"], "Excel RTD idle observation was shortened")
    for directory in directories[2:]:
        require(json.loads((directory / "host.exit.json").read_text()) == {"exit_code": 0, "finished": True}, "RTD host failed")
        require(not (directory / "host.stderr").read_text().strip(), "RTD host diagnostics")
        output = json.loads((directory / "host.stdout").read_text())
        require(output["live_callback_allocations"] == output["rtd_subscriptions_after_close"] == 0 and output["closed_registration"] == "#NAME?", "RTD teardown incomplete")
        if output.get("single_formula"):
            require([(r["formula"], r["value"]) for r in output["results"]] == [(c["formula"], c["value"]) for c in cases], "Host RTD outcomes differ")
        else:
            require([(r["formula"], r["value"], r["dependent"]) for r in output["results"]] == [(c["formula"], c["value"], c["dependent_value"]) for c in cases], "Host RTD outcomes differ")
        for case, row in zip(cases, output["results"], strict=True):
            if "rtd_idle_ms" in case:
                require(row["idle_observed_ms"] >= case["rtd_idle_ms"], "Host RTD idle observation was shortened")
        updates = output["rtd_updates"]
        if signaling:
            require(len(updates) >= 2 and [(v["sequence"], v["value"], v["disposition"]) for v in updates] == [(i + 1, None, "Accepted") for i in range(len(updates))], "Empty RTD signals were not admitted as ordered inputs")
            require(len({ident(v) for v in updates}) == 1, "Signaling RTD changed subscription identity")
        else:
            require([(v["sequence"], v["value"], v["disposition"]) for v in updates] == [(1, 1, "Accepted"), (2, 42, "Accepted")], "RTD admissions differ")
        require(output["results"][0]["provenance"]["rtd_inputs"] == [{**ident_fields(updates[-1]), "sequence": updates[-1]["sequence"]}], "RTD final certificate lacks its exact admitted input")
        require(not output["results"][-1]["provenance"]["rtd_inputs"], "Replacement retained an RTD input")
        require(output["rtd_retired"] == [{**ident_fields(updates[-1]), "disposition": "Accepted"}], "RTD retirement differs")
        require(not output["async_returns"] and not output["revoked_tokens"], "RTD used the async-completion route")
        outputs.append(output)
    traces = [boundary(p) for p in (excel, host)]
    protocols, raw_protocols, native, internals, notifications, server_bindings, ownership = [], [], [], [], [], [], []
    method_names = {10: "ServerStart", 11: "ConnectData", 12: "RefreshData", 13: "DisconnectData", 14: "Heartbeat", 15: "ServerTerminate"}
    for index, trace in enumerate(traces):
        protocol, inner, notices = [], [], []
        if signaling:
            starts = [e for e in trace[1].values() if e["entry"] == "COM!Invoke" and e.get("object_label") == "RTD server" and e.get("dispid") == 10 or e["entry"] == "COM!early.rtdserver.ServerStart" and e.get("from", {}).get("module") in HOST_MODULES]
            require(len(starts) == 1, "Signaling server startup was not observed once")
            start = starts[0]
            created = [e for e in trace[1].values() if (e["entry"] == "COM!CreateInstance" or e["entry"].endswith("!CoCreateInstance")) and e.get("iid") == "ec0e6191-db51-11d3-8f3e-00c04f3651b8" and trace[2][e["call"]].get("object") == start["object"] and trace[2][e["call"]]["seq"] < start["seq"]]
            require(created, "RTD server identity lacks explicit interface activation")
            creation = created[-1]
            enclosing = [e for e in trace[1].values() if e.get("function") == 379 and e["thread"] == creation["thread"] == start["thread"] and e["seq"] < creation["seq"] < trace[2][e["call"]]["seq"]]
            require(len(enclosing) == 1 and creation.get("xlfRtd_call", enclosing[0]["call"]) == enclosing[0]["call"], "Server activation is not bound to the actual RTD callback")
            callback_object = start["rgvarg"][0]["value"] if start["entry"] == "COM!Invoke" else start["callback"]
            server_bindings.append({"object": start["object"], "callback_object": callback_object, "creation": creation, "returned": trace[2][creation["call"]], "xlfRtd": enclosing[0]})
        for entry in trace[1].values():
            returned = trace[2][entry["call"]]
            dispatch = entry["entry"] == "COM!Invoke" and entry.get("object_label") == "RTD server"
            early = entry["entry"].startswith("COM!early.rtdserver.")
            if entry["entry"] == "COM!early.rtdsink.UpdateNotify":
                if signaling:
                    require(entry["object"] == callback_object, "RTD notification used another callback object")
                protocol.append({"method": "UpdateNotify", "hresult": returned["hresult"]})
                notices.append({"thread": entry["thread"], "from": entry["from"], "hresult": returned["hresult"]})
                continue
            if not dispatch and not early:
                continue
            if signaling:
                require(entry["object"] == server_bindings[-1]["object"], "RTD protocol switched server identity")
            # Excel's IDispatch implementation can forward into the same
            # object's early-bound method. Retain those bodies separately;
            # they are not a second external client request.
            parents = [e for e in trace[1].values() if e["entry"] == "COM!Invoke" and e.get("object_label") == "RTD server" and e["thread"] == entry["thread"] and e.get("object") == entry.get("object") and e["seq"] < entry["seq"] < returned["seq"] < trace[2][e["call"]]["seq"]]
            if early and parents:
                parent = min(parents, key=lambda e: e["seq"])
                method = entry["entry"].rsplit(".", 1)[-1]
                require(method_names.get(parent["dispid"]) == method, "Unexpected nested RTD method")
                outer = trace[2][parent["call"]]
                require(outer["hresult"] == returned["hresult"], "Internal RTD body changed its HRESULT")
                if method in ("ServerStart", "Heartbeat"):
                    require(outer["result"]["value"] == returned["started" if method == "ServerStart" else "value"], "Internal RTD scalar payload differs")
                elif method == "ConnectData":
                    require(outer["result"] == returned["result"] and outer["arguments_after"][0]["value"]["value"] == returned["new_values"], "Internal RTD ConnectData payload differs")
                elif method == "RefreshData":
                    require(outer["result"]["value"] == returned["data"] and outer["arguments_after"][0]["value"]["value"] == returned["count"], "Internal RTD refresh payload differs")
                inner.append({"entry": entry, "returned": returned})
                continue
            if early and not signaling and entry["entry"].endswith(".ServerTerminate") and entry.get("from", {}).get("module") in environments[0]["fixture_files"]:
                # This fixture's destructor calls its idempotent ServerTerminate
                # body while the client performs final Release. Keep the body,
                # but do not invent a second external ServerTerminate request.
                releases = [e for e in trace[1].values() if e["entry"] == "COM!Release" and e.get("object") == entry["object"] and e["thread"] == entry["thread"] and e["seq"] < entry["seq"] < returned["seq"] < trace[2][e["call"]]["seq"]]
                require(len(releases) == 1 and trace[2][releases[0]["call"]].get("references") == 0, "Internal RTD termination lacks the final owning Release")
                require(releases[0].get("from", {}).get("module") in (("EXCEL.EXE",) if index == 0 else HOST_MODULES), "Final RTD Release came from another owner")
                require(protocol and protocol[-1] == {"method":"ServerTerminate", "hresult":0} and returned["hresult"] == 0, "Destructor termination lacks prior successful client termination")
                inner.append({"entry":entry, "returned":returned, "reason":"final Release destructor", "release":releases[0], "release_returned":trace[2][releases[0]["call"]]})
                continue
            require(dispatch and entry["from"].get("module") == "EXCEL.EXE" if index == 0 else early and entry["from"].get("module") in HOST_MODULES, "Unexpected external RTD call transport/caller")
            method = method_names[entry["dispid"]] if dispatch else entry["entry"].rsplit(".", 1)[-1]
            row = {"method": method, "hresult": returned["hresult"]}
            arguments = entry.get("rgvarg", [])
            after = returned.get("arguments_after", [])
            if method == "ServerStart": row["started"] = returned["result"]["value"] if dispatch else returned["started"]
            elif method == "ConnectData":
                row.update({"topic": arguments[2]["value"] if dispatch else entry["topic"], "strings": arguments[1]["value"] if dispatch else entry["strings"], "new_before": arguments[0]["value"]["value"] if dispatch else entry["new_values"], "new_after": after[0]["value"]["value"] if dispatch else returned["new_values"], "initial": returned["result"]})
            elif method == "RefreshData": row.update({"count": after[0]["value"]["value"] if dispatch else returned["count"], "data": returned["result"]["value"] if dispatch else returned["data"]})
            elif method == "DisconnectData": row["topic"] = arguments[0]["value"] if dispatch else entry["topic"]
            elif method == "Heartbeat": row["value"] = returned["result"]["value"] if dispatch else returned["value"]
            protocol.append(row)
        if signaling:
            methods = [r["method"] for r in protocol]
            require(methods[:2] == ["ServerStart", "ConnectData"] and methods[-2:] == ["DisconnectData", "ServerTerminate"] and "UpdateNotify" in methods and "RefreshData" in methods, "Incomplete signaling RTD protocol")
        else:
            expected_methods = ["ServerStart", "ConnectData", "UpdateNotify", "RefreshData"]
            idle_intervals = [case.get("rtd_idle_ms", 0) for case in cases]
            if any(idle_intervals):
                require(idle_intervals == [22000, 0], "Numeric RTD idle interval has no qualified heartbeat expectation")
                expected_methods.append("Heartbeat")
            expected_methods += ["DisconnectData", "ServerTerminate"]
            require([r["method"] for r in protocol] == expected_methods, "Missing or additional RTD client calls")
        require(all(r["hresult"] == 0 for r in protocol), "RTD protocol call failed")
        if not signaling:
            require(protocol[3]["count"] == 1 and protocol[3]["data"]["values"] == [{"vt": 3, "value": 1}, {"vt": 5, "value": 42}] and not protocol[3]["data"]["truncated"], "RTD array data differs")
        raw_protocols.append(protocol)
        protocols.append(protocol); internals.append(inner); notifications.append(notices)
        records = all_invocations(trace, "X5.Rtd" if signaling else "X5.RtdValue", signaling)
        if signaling:
            ownership.append(framework_ownership(trace))
            require(records and all(not r["arguments"] for r in records), "Missing or changed signaling UDF entry")
            require(records[-1]["result"] == {"type": 0x4800, "value": 42}, "Signaling UDF never returned its final DLL-owned integer value")
            callbacks = [c for r in records for c in r["callbacks"] if c[0] == 379]
            require(callbacks and all(c[2] == 0 and len(c[1]) == 3 and c[1][1] == {"type": 2, "value": ""} for c in callbacks), "Signaling RTD callback failed or changed transport")
            require(all(c[1] == callbacks[0][1] for c in callbacks), "Signaling RTD changed provider/topic strings")
            expected_callback_results = [{"type": 1, "value": 0} for _ in callbacks]
            if index:
                expected_callback_results[0] = {"type": 16, "value": 42}
            require([c[3] for c in callbacks] == expected_callback_results, "Signaling RTD callback result types/values differ from the measured surface")
            connect = [r for r in protocol if r["method"] == "ConnectData"]
            require(len(connect) == 1 and connect[0]["initial"] == {"vt": 0, "value": None}, "Expected one empty signaling subscription")
            topic = connect[0]["topic"]
            topic_text = callbacks[0][1][2]["value"]
            require(connect[0]["strings"]["values"] == [{"vt": 8, "value": topic_text}], "RTD ConnectData differs from the actual callback topic")
            # Only per-session identities are normalized. All raw values and
            # every method call remain available above and in boundary.jsonl.
            normalized = json.loads(json.dumps(protocol))
            for row in normalized:
                if "topic" in row:
                    require(row["topic"] == topic, "RTD protocol used another topic")
                    row["topic"] = "<subscription>"
                if row["method"] == "ConnectData":
                    row["strings"]["values"][0]["value"] = "<async-topic>"
                if row["method"] == "RefreshData" and row["count"]:
                    data = row["data"]
                    require(data and data["dimensions"] == 2 and data["element_bytes"] == 24 and not data["truncated"] and len(data["values"]) == 2 * row["count"], "Incomplete RTD signal array")
                    for i in range(row["count"]):
                        require(data["values"][2*i] == {"vt": 3, "value": topic} and data["values"][2*i+1] == {"vt": 0, "value": None}, "RTD refresh was not this topic's empty signal")
                        data["values"][2*i]["value"] = "<subscription>"
            protocols[-1] = normalized
            if index == 1:
                updates = outputs[0]["rtd_updates"]
                require(ident(updates[0])[1] == topic and len(updates) == 1 + sum(r["count"] for r in protocol if r["method"] == "RefreshData"), "An observed empty RTD signal is missing from P07 admissions")
            native.append(records)
            continue
        for record in records:
            require(record["arguments"] == [] and len(record["callbacks"]) == 1, "Unexpected RTD UDF boundary")
            callback = record["callbacks"][0]
            require(callback[0] == 379 and callback[2] == 0, "RTD callback failed")
            require([a["value"] for a in callback[1]] == [f"DnaCalc.X5.{environments[0 if index == 0 else 2]['pid']}", "", "numeric"], "RTD callback topics/provider differ")
            require(record["result"] == callback[3], "Native RTD UDF changed the callback value")
        if any("X5.RtdValue" in c["formula"] for c in cases):
            expected = [{"type": 1, "value": 1}, {"type": 1, "value": 42}]
            # The host evaluates the function once more after it admits ConnectData's value as
            # sequence 1 (the in-step certificate repair refuses a result that read a sequence not
            # yet admitted), so its first answer is repeated. Excel's two answers are 1 then 42;
            # since dc-snz.23.3 the host's first answer is that 1, no longer a temporary #N/A.
            # OneCalc admits the connection's value after its evaluation instead (no certificate
            # repair, one formula), so it calls the function twice, as Excel does: 1 then 42.
            if index and not outputs[0].get("single_formula"): expected.insert(0, {"type": 1, "value": 1})
            require([r["result"] for r in records] == expected, "RTD native result sequence differs")
        else:
            require(not records, "A typed RTD formula makes no UDF call")
        native.append(records)
    if not signaling:
        require([sum(row.get("reason") == "final Release destructor" for row in rows) for rows in internals] == [1, 1], "Numeric RTD destructor observation missing or repeated")
    protocol_pairs = [{"position": i, "excel": a, "host": b, "matched": a == b} for i, (a, b) in enumerate(zip_longest(*protocols))]
    # Diagnose scheduling differences without relaxing the ordered verdict. Pair
    # each occurrence of each method, keeping every extra call as a failure.
    indexed = []
    for protocol in protocols:
        counts = Counter()
        calls = {}
        for position, row in enumerate(protocol):
            counts[row["method"]] += 1
            calls[(row["method"], counts[row["method"]])] = {"position": position, "payload": row}
        indexed.append(calls)
    method_pairs = [{"method": key[0], "occurrence": key[1], "excel": indexed[0].get(key), "host": indexed[1].get(key),
                     "matched": key in indexed[0] and key in indexed[1] and indexed[0][key]["payload"] == indexed[1][key]["payload"]}
                    for key in dict.fromkeys([*indexed[0], *indexed[1]])]
    publications = verify_sources(environments, publication_path)
    source_bindings_checked = sum(len(e["source_files"]) + 2 for e in environments)
    strict = all(r["matched"] for r in protocol_pairs)
    return {"status": "failed strict ordered RTD client protocol comparison" if not strict else "verified original xlOil signaling RTD protocol" if signaling else "verified controlled numeric RTD protocol", "formula_cases": len(cases), "matched_each_capture": len(cases), "controls_match": True,
            "client_calls_matched":sum(r["matched"] for r in protocol_pairs), "client_calls_failed":sum(not r["matched"] for r in protocol_pairs),
            "client_protocol_each_host": protocols[0], "raw_client_protocols_excel_and_host": raw_protocols, "client_protocol_comparison": protocol_pairs, "client_calls_by_method": method_pairs, "server_bindings": server_bindings, "native_invocations_excel_and_host": native, "native_ownership_excel_and_host": ownership, "internal_server_body_observations": internals, "notification_threads": notifications,
            "host_results": outputs, "source_publication_revisions": [p["revision"] for p in publications], "source_bindings_checked": sum(len(e["source_files"]) + 2 for e in environments),
            "transport": "Excel IDispatch vs host early-bound. Compare the external client method payloads; retain internal fixture body hits separately. SAFEARRAY bounds retain descriptor order.",
            "initial_difference": "The host's first RTD callback returns temporary #N/A before admission; subsequent empty signals return numeric zero as in Excel. All Python UDF invocations, defined ownership bits and callbacks are retained; invocation counts may differ with asynchronous publication." if signaling else "Host returns #N/A before the initial input is admitted, then1 and42; Excel returns1 then42. All raw invocations remain visible.",
            "scope": "Original xlOil 0.22.1 async generator and separate Python cache; empty sequenced inputs, exact P07 certificate, formula replacement and finite client protocol. General providers, arrays, remote servers and arbitrary teardown remain open." if signaling else "Controlled finite scalar RTD, exact P07 input certificate and retirement. General providers/teardown, arrays and remote RTD remain open."}


def spill_worker_packets(trace):
    """Reorder only the two proven independent worker calls; retain their raw order."""
    _, enters, leaves = trace
    native = [e for e in enters.values() if ":" in e["entry"]]
    names = Counter(e["entry"].rsplit(":", 1)[1] for e in native)
    require(names == Counter({"MyOverlapArray":4, "MyOverlap":4, "MyOverlapReset":1, "MyOverlapMax":2, "MyFreeCount":2}), "Spill native entries missing or additional")
    owner = next(e["thread"] for e in enters.values() if e["entry"].endswith("!xlAutoOpen"))
    reset = next(e for e in native if e["entry"].endswith(":MyOverlapReset"))
    roots = [e for e in native if e["entry"].endswith(":MyOverlapArray") and e["seq"] > leaves[reset["call"]]["seq"]]
    require(len(roots) == 2 and len({e["thread"] for e in roots}) == 2 and all(e["thread"] != owner for e in roots), "Spill packets lack two distinct workers")
    require(max(e["seq"] for e in roots) < min(leaves[e["call"]]["seq"] for e in roots), "Spill packet intervals do not overlap")
    packets, covered, frees = {}, set(), []
    for root in roots:
        record = invocation_record(trace, root)
        require(record["arguments"] in ([{"code":"E", "value":40}], [{"code":"E", "value":80}]), "Unexpected spill worker argument")
        key = record["arguments"][0]["value"]
        require(key not in packets and not record["callbacks"], "Repeated worker argument or unqualified worker callbacks")
        children = [e for e in native if e["thread"] == root["thread"] and root["seq"] < e["seq"] < leaves[e["call"]]["seq"] < leaves[root["call"]]["seq"]]
        require(len(children) == 1 and children[0]["entry"].endswith(":MyOverlap"), "Spill worker has missing or additional nested calls")
        child = invocation_record(trace, children[0])
        require(child == {"arguments":record["arguments"], "result":{"type":1,"value":key+1}, "callbacks":[]}, "Nested spill worker payload differs")
        returned = leaves[root["call"]]
        module = root["entry"].split("!", 1)[0]
        owned = [e for e in enters.values() if e["entry"] == module + "!xlAutoFree12" and e["thread"] == root["thread"] and e["oper_address"] == returned["rax"] and e["seq"] > returned["seq"]]
        require(len(owned) == 1 and leaves[owned[0]["call"]]["seq"] > owned[0]["seq"], "Spill worker lacks its unique same-pointer/thread/module free")
        require(not any(e["thread"] == root["thread"] and returned["seq"] < e["seq"] < owned[0]["seq"] for e in native), "Spill result survived another same-thread native entry")
        packets[key] = [root, children[0]]; covered.update(e["call"] for e in packets[key]); frees.append(owned[0])
    require(set(packets) == {40,80}, "Spill worker packet missing")
    outside = [e for e in native if e["call"] not in covered]
    require(all(e["thread"] == owner for e in outside), "Uncovered spill worker entry")
    first, last = min(e["seq"] for e in roots), max(leaves[e["call"]]["seq"] for e in frees)
    require(not any(first < e["seq"] < last for e in outside), "Owner entries interleaved with independent spill packets")
    canonical = [e for e in outside if e["seq"] < first] + packets[40] + packets[80] + [e for e in outside if e["seq"] > last]
    require(len(canonical) == len(native) and {e["call"] for e in canonical} == {e["call"] for e in native}, "Spill packet comparison lost or repeated native calls")
    return canonical, {"raw_native_entry_order":native, "independent_packets_by_argument":packets, "worker_frees":frees}


def verify_spill_pool(excel, excel_control, host, host_control, cases_path, publication_path):
    packet_orders = [spill_worker_packets(boundary(p)) for p in (excel, host)]
    result = verify_x2(excel, excel_control, host, host_control, cases_path, publication_path, x3=True, native_entry_order=[p[0] for p in packet_orders])
    cases = json.loads(cases_path.read_text(encoding="utf-8-sig"))
    for directory in (excel, excel_control):
        control = controller(directory)
        for case in cases:
            if "dependent" in case:
                require(control["dependent " + case["formula"]] == case["dependent_value"], "Excel independent spill anchor differs")
            for address, expected in case.get("spill_cells", {}).items():
                require(control["spill " + case["formula"] + " " + address] == expected, "Excel spill cell differs")
    for directory in (host, host_control):
        output = json.loads((directory / "host.stdout").read_text())
        for row, case in zip(output["results"], cases, strict=True):
            if "dependent" in case:
                require(row["dependent"] == case["dependent_value"], "Host independent spill anchor differs")
            require(set(row["spill_cells"]) == set(case.get("spill_cells", {})), "Host spill probes missing or additional")
            for address, expected in case.get("spill_cells", {}).items():
                probe = row["spill_cells"][address]
                require(probe["value"] == expected and probe["provenance"]["completed_invocations"] == probe["provenance"]["cooperative_invocations"] == 1, "Spill cell lacks its actual producing native computation")
    witnesses, ordered = [], []
    for directory in (excel, host):
        trace = boundary(directory); _, enters, leaves = trace
        owner = next(e["thread"] for e in enters.values() if e["entry"].endswith("!xlAutoOpen"))
        reset = next(e for e in enters.values() if e["entry"].endswith(":MyOverlapReset"))
        array_calls = [e for e in enters.values() if e["entry"].endswith(":MyOverlapArray")]
        entry_calls = [e for e in array_calls if e["seq"] < reset["seq"]]
        workers = [e for e in array_calls if e["seq"] > leaves[reset["call"]]["seq"]]
        require(len(entry_calls) == len(workers) == 2 and all(e["thread"] == owner for e in entry_calls), "Spill entry/calculation phases differ")
        require(len({e["thread"] for e in workers}) == 2 and all(e["thread"] != owner for e in workers), "Spill producers did not use two workers")
        require(max(e["seq"] for e in workers) < min(leaves[e["call"]]["seq"] for e in workers), "Native spill producers did not overlap")
        ownership = framework_ownership(trace)
        require(ownership["owned_returns"] == ownership["free_calls"] == 3, "Native spill ownership/free count differs")
        witnesses.append({"owner_thread": owner, "worker_entries": workers, "ownership": ownership})
        ordered.append([{"name":e["entry"].rsplit(":",1)[1], **invocation_record(trace,e)} for e in enters.values() if ":" in e["entry"]])
    canonical = [[{"name":e["entry"].rsplit(":",1)[1], **invocation_record(boundary(p),e)} for e in packet[0]] for p, packet in zip((excel,host), packet_orders, strict=True)]
    require(canonical[0] == canonical[1], "Spill owner order or independent worker payloads differ")
    result["spill_total_entry_order_matches"] = ordered[0] == ordered[1]
    result["spill_raw_native_calls_excel_and_host"] = ordered
    result["spill_worker_packets_excel_and_host"] = [packet[1] for packet in packet_orders]
    if not result["status"].startswith("failed"):
        result["status"] = "verified concurrent native spills with independent worker interleavings"
    result["spill_pool"] = {"witnesses": witnesses, "cells_each_capture": sum(len(c.get("spill_cells", {})) for c in cases), "scope": "Two native computations overlap; owner entry and worker calculation, every selected spill cell, all owner calls in order, each independent worker packet by its argument and same-module/pointer/thread frees agree. Raw cross-worker entry order stays explicit. Portable tests separately check competing placement order and completed-output capacity."}
    return result


def verify_workbook_pool(excel, excel_control, host, host_control, cases_path, publication_path):
    if any(c.get("spill_cells") for c in json.loads(cases_path.read_text(encoding="utf-8-sig"))):
        return verify_spill_pool(excel, excel_control, host, host_control, cases_path, publication_path)
    directories = (excel, excel_control, host, host_control)
    envs = [json.loads((p / "environment.json").read_text(encoding="utf-8-sig")) for p in directories]
    cases = json.loads(cases_path.read_text(encoding="utf-8-sig"))
    require(all(c.get("manual") and c.get("legacy") for c in cases), "Pool cases must select the same legacy/manual calculation form")
    require(all(e["fixture_files"] == envs[0]["fixture_files"] and e["cases_sha256"] == digest(cases_path) for e in envs), "Different pool fixture/cases")
    require(envs[0]["excel_file_version"] == envs[1]["excel_file_version"] and envs[0]["tracer_sha256"] == envs[2]["tracer_sha256"], "Different pool Excel builds/tracers")
    require(envs[2]["executable_sha256"] == envs[3]["executable_sha256"] and envs[2]["shim_sha256"] == envs[3]["shim_sha256"], "Different pool host binaries")
    outcomes = []
    for directory, env in zip(directories[:2], envs[:2]):
        data = controller(directory)
        require(env["pid"] != 3144 and env["pid"] not in env["preexisting_excel_pids"] and data["AutomationSecurity unchanged"], "Pool Excel ownership/security failed")
        for case in cases:
            outcomes.append({"capture":directory.name,"formula":case["formula"],"expected":case["value"],"actual":data[case["formula"]],"matched":data[case["formula"]] == case["value"]})
            if "dependent" in case:
                require(data["dependent " + case["formula"]] == case["dependent_value"], "Excel second pool cell differs")
    outputs = []
    for directory in directories[2:]:
        require(json.loads((directory / "host.exit.json").read_text()) == {"exit_code": 0, "finished": True}, "Pool host failed")
        require(not (directory / "host.stderr").read_text().strip(), "Pool host diagnostics")
        output = json.loads((directory / "host.stdout").read_text())
        require(output["host"] == "DNA Calc / CalcHost P07" and output["live_callback_allocations"] == 0 and output["closed_registration"] == "#NAME?", "Pool owner/teardown differs")
        for case, row in zip(cases, output["results"], strict=True):
            require((row["formula"], row["value"]) == (case["formula"], case["value"]), "Host pool result differs")
            if "dependent" in case:
                require(row["dependent"] == case["dependent_value"], "Host second pool cell differs")
        for proof in (output["results"][0]["provenance"], output["results"][0]["dependent_provenance"]):
            require(proof["cooperative_invocations"] == proof["completed_invocations"] == 1, "Pool cell lacks its single owning activation")
        require(not output["async_returns"] and not output["revoked_tokens"], "Scalar pool used native async completion")
        outputs.append(output)
    forms = {"MyOverlap":"QE$", "MyOverlapMax":"J", "MyOverlapReset":"J#", "MyWorkerCallbacks":"QQQ$", "MyWorkerReference":"QU$", "MyWorkerOwned":"Q$", "MyFreeCount":"J"}
    traces = [boundary(p) for p in (excel, host)]
    records, overlaps, ownership, entry_counts, worker_calls = [], [], [], [], []
    for trace in traces:
        registrations = {e["entry"].rsplit(":", 1)[-1]:e["signature"] for e in trace[0] if e["event"] == "registered_entry"}
        require(all(registrations.get(name) == form for name, form in forms.items()), "Pool signatures differ")
        opens = [e for e in trace[1].values() if e["entry"].endswith("!xlAutoOpen")]
        require(len(opens) == 1, "Pool loader entry missing/repeated")
        owner = opens[0]["thread"]
        entries = {name:[e for e in trace[1].values() if e["entry"].endswith(":" + name)] for name in forms}
        require(all(entries.values()) and len(entries["MyOverlapReset"]) == 1, "Pool native invocation/reset missing")
        reset = entries["MyOverlapReset"][0]
        require(reset["thread"] == owner and trace[2][reset["call"]]["result"] == 1, "Pool barrier reset failed")
        pair = [e for e in entries["MyOverlap"] if e["seq"] > trace[2][reset["call"]]["seq"]]
        require(len(pair) == 2, "Calculation did not enter exactly two barrier calls after reset")
        for name, native in entries.items():
            if forms[name].endswith("$"):
                require(any(e["thread"] != owner for e in native), "No calculation worker observed for " + name)
            else:
                require(all(e["thread"] == owner for e in native), "Serial/macro pool entry left its owner")
        require(pair[0]["thread"] != pair[1]["thread"] and all(e["thread"] != owner for e in pair), "Pool barriers did not enter distinct workers")
        require(pair[0]["seq"] < trace[2][pair[1]["call"]]["seq"] and pair[1]["seq"] < trace[2][pair[0]["call"]]["seq"], "Pool native entry intervals never overlapped")
        measured = {name:sorted([{**invocation_record(trace, e),"execution_context":"owner" if e["thread"] == owner else "worker"} for e in native], key=lambda r:json.dumps(r["arguments"], sort_keys=True)) for name, native in entries.items()}
        calculation_calls = sorted([invocation_record(trace,e) for e in pair],key=lambda r:json.dumps(r["arguments"],sort_keys=True))
        require([r["arguments"] for r in calculation_calls] == [[{"code":"E","value":40}], [{"code":"E","value":80}]], "Pool barrier arguments differ")
        require([r["result"] for r in calculation_calls] == [{"type":1,"value":41},{"type":1,"value":81}], "Pool barrier did not observe both entries")
        worker_calls.append(calculation_calls)
        entry_counts.append({name:len(native) for name,native in entries.items()})
        records.append(measured)
        overlaps.append({"owner_thread":owner,"entries":pair,"returns":[trace[2][e["call"]] for e in pair]})
        ownership.append(framework_ownership(trace))
    verify_sources(envs, publication_path)
    source_bindings_checked = sum(len(e["source_files"])+2 for e in envs)
    failures = []
    if not all(r["matched"] for r in outcomes): failures.append("Excel formula outcomes differ")
    if entry_counts[0] != entry_counts[1]: failures.append("Native invocation counts differ")
    if records[0] != records[1]: failures.append("Native arguments/results/ordered nested callbacks differ")
    expected_entries = {"MyOverlap":4, "MyOverlapMax":2, "MyOverlapReset":1, "MyWorkerCallbacks":2, "MyWorkerReference":2, "MyWorkerOwned":2, "MyFreeCount":2}
    if not all(counts == expected_entries for counts in entry_counts): failures.append("Entry and calculation occurrences missing or repeated")
    if not all(o["owned_returns"] == o["free_calls"] == 2 for o in ownership): failures.append("Owned result/free counts differ")
    require(worker_calls[0] == worker_calls[1], "Calculation-phase barrier calls differ")
    require(not failures, "; ".join(failures))
    return {"status":"verified within-workbook legacy scalar pool", "formula_cases":len(cases), "native_invocations_each_host":sum(entry_counts[0].values()),
            "native_records":records[0], "overlap_excel_and_host":overlaps,"ownership_excel_and_host":ownership,"host_results":outputs,
            "source_bindings_checked":sum(len(e["source_files"])+2 for e in envs),
            "scope":"Two independent explicitly legacy scalar cells in one workbook, distinct worker entries, owner-serviced callbacks, exact P07 activations and same-thread frees. Dynamic spill formulas stay ordered; volatile plans and additional dynamic callback prerequisites are outside this concurrent route."}


def verify_permissions(excel, control, hosts, cases_path, publication_path):
    cases = json.loads(cases_path.read_text(encoding="utf-8-sig"))
    environments = [json.loads((p / "environment.json").read_text(encoding="utf-8-sig")) for p in (excel, control)]
    trace = boundary(excel)
    require(environments[0]["fixture_files"] == environments[1]["fixture_files"], "Different permission control fixture binaries")
    require(environments[0]["excel_file_version"] == environments[1]["excel_file_version"], "Different permission Excel builds")
    observed = []
    for directory, env in zip((excel, control), environments):
        data = controller(directory)
        require(data["AutomationSecurity unchanged"] and env["pid"] != 3144 and env["pid"] not in env["preexisting_excel_pids"], "Permission control ownership/security failed")
        observed.append([(r["operation"], r["value"]) for r in jsonl(directory / "controller.jsonl") if r["operation"].startswith("=")])
        require(env["cases_sha256"] == digest(cases_path), "Permission cases changed")
    require(observed[0] == observed[1] and [r[0] for r in observed[0]] == [c["formula"] for c in cases], "Excel permission controls differ or lack rows")
    measured = [entry for entry in trace[1].values() if any(entry["entry"].endswith(":" + name) for name in ("X5.Permission", "X5.MacroPermission", "X5.PoolPermission"))]
    require(len(measured) == len(cases), "Missing permission native invocations")
    opens = [entry for entry in trace[1].values() if entry["entry"] == "X5Native64.xll!xlAutoOpen"]
    require(len(opens) == 1, "Missing or repeated Excel permission initialization")
    excel_owner = opens[0]["thread"]
    forms = {"X5.Permission": "JB", "X5.MacroPermission": "JB#", "X5.PoolPermission": "JB$"}
    registrations = {entry["entry"].rsplit(":", 1)[-1]: entry["signature"] for entry in trace[0] if entry["event"] == "registered_entry"}
    require(all(registrations.get(name) == form for name, form in forms.items()), "Excel permission registration flags differ")
    rows = []
    for index, (case, entry) in enumerate(zip(cases, measured)):
        function = int(re.search(r"\((\d+)\)", case["formula"])[1])
        name = case["formula"].split("(", 1)[0].lstrip("=")
        require(entry["entry"].endswith(":" + name) and entry["args"] == [{"code": "B", "value": function}], "Excel permission function/argument differs")
        require((entry["thread"] != excel_owner) == (name == "X5.PoolPermission"), "Excel permission context used the wrong thread")
        callbacks = [e for e in trace[1].values() if e.get("function") == function and entry["seq"] < e["seq"] < trace[2][entry["call"]]["seq"] and e["thread"] == entry["thread"]]
        require(len(callbacks) == 1, "Permission callback was not observed once")
        excel_code = trace[2][callbacks[0]["call"]]["return_code"]
        require(excel_code == observed[0][index][1], "Permission UDF changed Excel callback status")
        directory = hosts / f"{index:03d}"
        env = json.loads((directory / "environment.json").read_text(encoding="utf-8-sig"))
        require(env["fixture_files"] == environments[0]["fixture_files"], "Different permission fixture binary")
        packet = hosts / "cases" / f"{index:03d}.json"
        require(env["cases_sha256"] == digest(packet) and json.loads(packet.read_text(encoding="utf-8-sig")) == [case], "Host permission request packet changed")
        environments.append(env)
        log = [line.split("\t") for line in (directory / "rtd.tsv").read_text(encoding="utf-8-sig").splitlines()]
        codes = [int(r[2]) for r in log if r[0] == "PermissionCode"]
        functions = [int(r[2]) for r in log if r[0] == "PermissionFunction"]
        require(len(codes) == 1 and functions == [function], "Host permission read missing or repeated")
        owner = int(next(r[1] for r in log if r[0] == "PrivateClassRegistered"))
        invocation = int(next(r[1] for r in log if r[0] == "PermissionContext"))
        require((invocation != owner) == ("PoolPermission" in case["formula"]), "Host permission context used the wrong thread")
        status = json.loads((directory / "host.exit.json").read_text(encoding="utf-8-sig"))
        error = (directory / "host.stderr").read_text(encoding="utf-8-sig")
        calculated = status == {"exit_code": 0, "finished": True}
        evaluated_payload = None
        if calculated:
            output = json.loads((directory / "host.stdout").read_text(encoding="utf-8-sig"))
            require(output["results"][0]["formula"] == case["formula"], "Host calculated a different permission formula")
            require(not error.strip() and output["results"][0]["value"] == codes[0] and output["live_callback_allocations"] == 0 and output["closed_registration"] == "#NAME?", "Invalid permission calculation/teardown")
            if function == 32890:
                require(output["status_message"] == ("X5 permission" if "MacroPermission" in case["formula"] else None), "Permission call changed the wrong message state")
            if function == 257 and excel_code == 0:
                types = [int(r[2]) for r in log if r[0] == "EvaluateResultType"]
                bits = [r[2] for r in log if r[0] == "EvaluateResultBits"]
                require(len(types) == len(bits) == 1 and re.fullmatch(r"[0-9a-f]{16}", bits[0]), "EVALUATE result payload was not retained once")
                evaluated_payload = {"type": types[0] & 0xfff, "value": struct.unpack(">d", bytes.fromhex(bits[0]))[0]}
                excel_result = trace[2][callbacks[0]["call"]]["result"]
                require(evaluated_payload == {"type": excel_result["type"] & 0xfff, "value": excel_result["value"]}, "EVALUATE result type/value differs")
        else:
            require(status == {"exit_code": 1, "finished": False} and "NotModelled" in error and "dc-snz.2" in error, "Unexpected permission process failure")
        rows.append({"formula": case["formula"], "function": function, "registration_form": forms[name], "excel_owner_thread": excel_owner, "excel_invocation_thread": entry["thread"], "excel_code": excel_code, "host_code": codes[0], "callback_code_match": excel_code == codes[0], "host_calculated": calculated, "matched_formula": calculated and excel_code == codes[0], "host_error": error.strip() or None, "excel_callback": {"entry": callbacks[0], "returned": trace[2][callbacks[0]["call"]]}, "host_evaluate_payload": evaluated_payload, "host_owner_thread": owner, "host_invocation_thread": invocation})
    require(len({env["executable_sha256"] for env in environments[2:]}) == 1, "Permission host executable changed during the batch")
    require(len({env["shim_sha256"] for env in environments[2:]}) == 1, "Permission companion binary changed during the batch")
    verify_sources(environments, publication_path)
    matched = sum(r["matched_formula"] for r in rows)
    return {"status": "measured permission matrix; refusals remain failures", "requests": len(rows), "pass": matched, "fail": len(rows) - matched, "pass_rate_all_requests": matched / len(rows), "callback_codes_matched": sum(r["callback_code_match"] for r in rows), "rows": rows,
            "source_bindings_checked": sum(len(e["source_files"]) + 2 for e in environments),
            "scope": "13 callback requests across normal, # and $ registrations. Excel boundaries are traced and controlled; each host request uses an independent real executable and records its actual callback return code. The two successful EVALUATE result types and numeric bits are also compared. Broader expressions remain outside these requests; message state is checked, UI painting is unmeasured."}


# C.1 of answer 13: every XLL comparison states its profile, and reports four counts side by
# side, each as pass = matched Excel and fail = mismatched + refused + missing, so 289 own-fixture
# value cases never hide a failing boundary, lifecycle or framework lane.
COUNT_LANES = ("value_cases", "ordered_boundary_cases", "lifecycle_ownership_cases", "framework_feature_lanes")


def comparison_manifest(excel_env, controller_rows, cases, host_envs):
    """The comparison profile of one Excel run against its host runs (C.1)."""
    modes = []
    for row in controller_rows:
        if row["operation"].startswith("calculation mode ") and row["value"] not in modes:
            modes.append(row["value"])
    manual = any(case.get("manual") for case in cases)
    return {"excel_build": excel_env.get("excel_file_version"), "bitness": excel_env.get("process_bits"),
            "locale": excel_env.get("culture"), "calculation_modes": modes,
            "mtr": {"enabled": excel_env.get("multi_threaded_enabled"), "threads": excel_env.get("multi_thread_count")},
            "trigger": "Formula2 entry into one cell" + (", then Application.Calculate for manual cases" if manual else ", automatic recalculation"),
            "addin_files": excel_env.get("fixture_files"), "workbook": excel_env.get("workbook_name"),
            "load_order": [row["operation"] for row in controller_rows if row["operation"].startswith("RegisterXLL")],
            "excel_mode": excel_env.get("mode"),
            "host_executables": sorted({e.get("executable_sha256") for e in host_envs if e.get("executable_sha256")}),
            "host_process_identity": sorted({e.get("assume_excel_process") for e in host_envs if e.get("assume_excel_process")}),
            "repository_commits": sorted({e.get("repository_commit") for e in [excel_env, *host_envs] if e.get("repository_commit")})}


def four_counts(rows):
    """Rows carry `lane` and `outcome` (pass, mismatched, refused, missing)."""
    counts = {lane: {"pass": 0, "mismatched": 0, "refused": 0, "missing": 0} for lane in COUNT_LANES}
    for row in rows:
        counts[row["lane"]][row["outcome"]] += 1
    for count in counts.values():
        total = sum(count.values())
        count.update(total=total, fail=total - count["pass"], pass_rate=count["pass"] / total if total else None)
    return counts


def verify_boundary(excel, hosts, cases_path, repeat=None):
    """dc-6332.10 boundary batch: the probes report callback codes and output writes as text.

    One untraced Excel control runs every case; each host group ran in its own OneCalc process
    (run-batch.ps1). A host refusal is a failure, never a match; Excel's answers are the oracle.
    """
    cases = json.loads(cases_path.read_text(encoding="utf-8-sig"))
    env = json.loads((excel / "environment.json").read_text(encoding="utf-8-sig"))
    observed = controller(excel)
    # The case file gains Excel's recorded answers after the run, so bind the formulas, in order.
    formulas = [r["operation"] for r in jsonl(excel / "controller.jsonl") if r["operation"].startswith("=")]
    require(formulas == [c["formula"] for c in cases], "Boundary case formulas differ from the Excel run")
    # The lane's session host owns the Excel process (session-receipt.json), so it is running before the job.
    owned = (observed.get("cleanup") == "Workbook closed; Excel returned to lane session host"
             or env["pid"] not in env["preexisting_excel_pids"])
    require(observed["AutomationSecurity unchanged"] is True and env["pid"] != 3144 and owned, "Excel ownership/security failed")
    host_values = {}
    host_envs = []
    for directory in sorted(p for p in hosts.iterdir() if p.is_dir() and p.name != "cases"):
        packet = json.loads((hosts / "cases" / f"{directory.name}.json").read_text(encoding="utf-8-sig"))
        host_env = json.loads((directory / "environment.json").read_text(encoding="utf-8-sig"))
        require(host_env["fixture_files"] == env["fixture_files"], "Host and Excel loaded different fixture files")
        host_envs.append(host_env)
        status = json.loads((directory / "host.exit.json").read_text(encoding="utf-8-sig"))
        error = (directory / "host.stderr").read_text(encoding="utf-8-sig").strip()
        if status == {"exit_code": 0, "finished": True}:
            output = json.loads((directory / "host.stdout").read_text(encoding="utf-8-sig"))
            for row in output["results"]:
                host_values[row["formula"]] = {"value": row["value"], "refused": None}
        else:
            # The case runner stops at its first failure: a value that differs from the
            # recorded answer is a mismatch with the host's value, anything else a refusal.
            # Cases before the failing one passed the runner's own check against their recorded
            # value; cases after it never ran (missing).
            differs = re.fullmatch(r"dna-onecalc-desktop: (=.*): expected .*, calculated (.*)", error)
            failing = re.match(r"dna-onecalc-desktop: (=[^:]*\)):", error)
            failing = failing[1] if failing else None
            reached = False
            for case in packet:
                if case["formula"] == failing:
                    reached = True
                    host_values[case["formula"]] = ({"value": json.loads(differs[2]), "refused": None} if differs
                                                     else {"value": None, "refused": error})
                elif failing and not reached and case.get("value") is not None:
                    host_values[case["formula"]] = {"value": case["value"], "refused": None}
                elif failing and reached:
                    continue
                else:
                    host_values[case["formula"]] = {"value": None, "refused": error or json.dumps(status)}
    # Value2 reports a worksheet error as its COM error code; the host reports {"error": name}.
    excel_errors = {-2146826288: "#NULL!", -2146826281: "#DIV/0!", -2146826273: "#VALUE!", -2146826265: "#REF!",
                    -2146826259: "#NAME?", -2146826252: "#NUM!", -2146826246: "#N/A", -2146826245: "#GETTING_DATA"}
    def comparable(value):
        return {"error": excel_errors[value]} if isinstance(value, int) and value in excel_errors else value
    rows = []
    for case in cases:
        formula = case["formula"]
        require(formula in observed, "Excel did not record " + formula)
        host = host_values.get(formula)
        matched = host is not None and host["refused"] is None and comparable(host["value"]) == comparable(observed[formula])
        outcome = ("missing" if host is None else "refused" if host["refused"] is not None
                   else "pass" if matched else "mismatched")
        # The MyBoundary probes are callback boundary cases; the free, retain and open probes
        # are ownership and lifecycle cases.
        lane = "ordered_boundary_cases" if formula.startswith("=MyBoundary") and "Open" not in formula else "lifecycle_ownership_cases"
        rows.append({"formula": formula, "lane": lane, "outcome": outcome, "excel": observed[formula],
                     "host": None if host is None else host["value"], "host_refusal": None if host is None else host["refused"],
                     "matched": matched, "recorded_value_agrees": case.get("value") is None or comparable(case["value"]) == comparable(observed[formula])})
    matched = sum(r["matched"] for r in rows)
    refused = sum(r["outcome"] == "refused" for r in rows)
    missing = sum(r["outcome"] == "missing" for r in rows)
    require(all(r["recorded_value_agrees"] for r in rows), "A recorded case value differs from Excel's answer")
    result = {"status": "measured boundary batch; refusals remain failures", "cases": len(rows), "pass": matched,
              "fail": len(rows) - matched, "mismatched": len(rows) - matched - refused - missing, "refused": refused,
              "missing": missing, "pass_rate_all_cases": matched / len(rows), "counts": four_counts(rows),
              "manifest": comparison_manifest(env, jsonl(excel / "controller.jsonl"), cases, host_envs), "rows": rows,
              "scope": "Callback status, thread placement and output writes as reported by the fixture's own probes (sentinel-filled result operands); not a raw debugger trace."}
    if repeat:
        again = controller(repeat)
        differ = [c["formula"] for c in cases if again.get(c["formula"]) != observed[c["formula"]]]
        result["excel_repeat"] = {"cases": len(cases), "agree": len(cases) - len(differ), "differ": differ,
                                  "scope": "A second Excel run of the same cases and binaries; a differing row is a nondeterministic Excel answer, never a host pass."}
    return result


def verify_ordered(excel, host, cases_path, repeat=None):
    """dc-6332.11 status 64 batch (x17): one case list run in Excel (run.ps1) and in the engine
    (`dna-calc-desktop --engine --cases`, its stdout in `host/host.stdout.json`). Formulas repeat
    (each group ends with `=MyUcLog()`), so rows compare by position, not by formula. A host
    stop is a refusal for its case and missing for the rest; Excel's answers are the oracle."""
    cases = json.loads(cases_path.read_text(encoding="utf-8-sig"))
    env = json.loads((excel / "environment.json").read_text(encoding="utf-8-sig"))
    rows_excel = jsonl(excel / "controller.jsonl")
    observed = controller(excel)
    answers = [r["value"] for r in rows_excel if r["operation"].startswith("=")]
    formulas = [r["operation"] for r in rows_excel if r["operation"].startswith("=")]
    require(formulas == [c["formula"] for c in cases], "Ordered case formulas differ from the Excel run")
    owned = (observed.get("cleanup") == "Workbook closed; Excel returned to lane session host"
             or env["pid"] not in env["preexisting_excel_pids"])
    require(observed["AutomationSecurity unchanged"] is True and env["pid"] != 3144 and owned, "Excel ownership/security failed")
    host_env = json.loads((host / "environment.json").read_text(encoding="utf-8-sig"))
    require(host_env["fixture_files"]["X1.xll"] == env["fixture_files"]["X1.xll"], "Host and Excel loaded different fixture binaries")
    status = json.loads((host / "host.exit.json").read_text(encoding="utf-8-sig"))
    output = json.loads((host / "host.stdout.json").read_text(encoding="utf-8-sig")) if status.get("exit_code") == 0 else {"results": []}
    error = (host / "host.stderr").read_text(encoding="utf-8-sig").strip() if (host / "host.stderr").exists() else ""
    values = [row["value"] for row in output["results"]]
    rows = []
    for index, (case, excel_value) in enumerate(zip(cases, answers)):
        if index < len(values):
            host_value = values[index]
            same = host_value == excel_value or (isinstance(host_value, (int, float)) and isinstance(excel_value, (int, float))
                                                 and float(host_value) == float(excel_value))
            outcome = "pass" if same else "mismatched"
        else:
            host_value = None
            outcome = "refused" if index == len(values) and status.get("exit_code") != 0 else "missing"
        lane = "ordered_boundary_cases" if case["formula"] in ("=MyUcLog()", "=MyUcThreads()") else "value_cases"
        rows.append({"index": index, "group": case.get("group"), "formula": case["formula"], "lane": lane, "outcome": outcome,
                     "excel": excel_value, "host": host_value, "host_error": error if outcome == "refused" else None,
                     "recorded_value_agrees": case.get("value") is None or case["value"] == excel_value
                     or (isinstance(case["value"], (int, float)) and isinstance(excel_value, (int, float)) and float(case["value"]) == float(excel_value))})
    matched = sum(r["outcome"] == "pass" for r in rows)
    refused = sum(r["outcome"] == "refused" for r in rows)
    missing = sum(r["outcome"] == "missing" for r in rows)
    result = {"status": "measured ordered batch; refusals remain failures", "cases": len(rows), "pass": matched,
              "fail": len(rows) - matched, "mismatched": len(rows) - matched - refused - missing, "refused": refused,
              "missing": missing, "pass_rate_all_cases": matched / len(rows), "counts": four_counts(rows),
              "recorded_values_agree": all(r["recorded_value_agrees"] for r in rows),
              "manifest": comparison_manifest(env, rows_excel, cases, [host_env]), "rows": rows,
              "scope": "The fixture's own ordered log of entries, xlCoerce statuses, result-operand writes and xlAutoFree12 calls; the log rows are the ordered boundary lane, the cell values the other."}
    if repeat:
        again = [r["value"] for r in jsonl(repeat / "controller.jsonl") if r["operation"].startswith("=")]
        differ = [i for i, (a, b) in enumerate(zip(answers, again)) if a != b] + list(range(len(again), len(answers)))
        result["excel_repeat"] = {"cases": len(answers), "agree": len(answers) - len(differ), "differ": differ,
                                  "scope": "A second Excel run of the same cases and binaries; a differing row is a nondeterministic Excel answer, never a host pass."}
    return result


def verify_x2_controls(excel, host, identity=False):
    """Additional controls, kept separate from traced UDF and corpus match counts."""
    envs = [json.loads((p / "environment.json").read_text(encoding="utf-8-sig")) for p in (excel, host)]
    require(envs[0]["fixture_files"] == envs[1]["fixture_files"], "Different control fixture binaries")
    require(envs[0]["cases_sha256"] == envs[1]["cases_sha256"], "Different control cases")
    require(envs[0]["pid"] != 3144 and envs[0]["pid"] not in envs[0]["preexisting_excel_pids"], "Unowned Excel PID")
    observed = controller(excel)
    output = json.loads((host / "host.stdout").read_text(encoding="utf-8-sig"))
    require(observed["AutomationSecurity unchanged"] is True, "Security setting changed")
    require(not (host / "host.stderr").read_text(encoding="utf-8-sig").strip(), "Host reported diagnostics")
    require(output["live_callback_allocations"] == 0 and output["closed_registration"] == "#NAME?", "Control did not release its host registrations/storage")
    actual = {r["formula"]: r["value"] for r in output["results"]}
    if identity:
        require(observed['=SayHello("x")'] == actual['=SayHello("x")'] == "Hello, x", "Hello control differs")
        values = [observed["=X2.Identity()"], actual["=X2.Identity()"]]
        parsed = []
        for index, value in enumerate(values):
            fields = value.split("|")
            require(len(fields) == 9, "Incomplete identity probe")
            process, major, assume, default_domain, version, hwnd, result, command_thread, function_thread = fields
            require(process.lower() == "excel" if index == 0 else process.lower() in ("dna-calc-desktop", "dna-onecalc-desktop"), "Process was renamed")
            require(major == "16" and version == "16.0" and int(hwnd) != 0, "Process or COM identity differs")
            require(result in ("True", "False") and command_thread == function_thread and int(command_thread) != 0, "PIA Run did not execute on the main lane")
            if index == 1:
                require(assume == "1", "Host environment opt-in missing")
                if "net48" in envs[0]["fixture_framework"]:
                    require(default_domain == "False", "New loader did not isolate the Framework add-in")
            parsed.append({"process": process, "file_major": major, "assume_excel_process": assume,
                           "default_appdomain": default_domain, "com_version": version,
                           "com_hwnd": int(hwnd), "pia_run_result": result, "command_and_function_thread": int(command_thread)})
        require(output["reload_results"] == output["results"], "PIA/identity behavior changed on reload")
        require(parsed[0]["pia_run_result"] == parsed[1]["pia_run_result"], "PIA native command return differs")
        return {"status": "verified process identity and original PIA calls", "excel_and_host": parsed,
                "same_fixture_files": envs[0]["fixture_files"], "reload_checked": True,
                "host_executable_sha256": envs[1]["executable_sha256"],
                "scope": "Control without debugger; Version, Hwnd and Run through ExcelDna.Interop 15.0.1; process identities intentionally differ."}
    expected = {"=X2.Pool0(3)": 3, "=X2.Pool9999(3)": 10002}
    require(actual == expected and {k: v for k, v in observed.items() if k.startswith("=")} == expected, "Export pool endpoint values differ")
    require(output["registrations"] == 10002 and output["numbered_exports"] == [0, 9999], "Full original numbered export pool was not registered")
    return {"status": "verified original 10000-export pool", "registered_numbered_exports": 10000,
            "total_registrations": output["registrations"], "tested_endpoints": expected,
            "same_fixture_files": envs[0]["fixture_files"], "host_executable_sha256": envs[1]["executable_sha256"],
            "scope": "No-debugger controls; every registration resolved an original export; values checked at f0 and f9999, not all 10000 delegates."}


def verify_ribbon(excel, excel_control, host, host_control, cases_path, publication_path=None):
    result = verify_x2(excel, excel_control, host, host_control, cases_path, publication_path, x3=True)
    traces = [boundary(p) for p in (excel, host)]
    startup, teardown, property_calls, traffic, bindings = [], [], [], [], []
    for trace in traces:
        _, enters, leaves = trace
        opening = next(e for e in enters.values() if e["entry"].endswith("!xlAutoOpen"))
        closed = min(e["seq"] for e in enters.values() if e["entry"].endswith(("!xlAutoRemove", "!xlAutoClose")))
        starts, ends, properties = [], [], []
        factories = [e for e in enters.values() if e["entry"] == "COM!CreateInstance"
                     and e.get("iid") == "b65ad801-abaf-11d0-bb8b-00a0c90f2744"
                     and leaves[e["call"]]["hresult"] == 0]
        objects = {leaves[e["call"]]["object"] for e in factories}
        require(len(objects) == 2, "Dummy and real COM add-in activation were not observed")
        for entry in enters.values():
            returned = leaves[entry["call"]]
            name = entry["entry"]
            if name.startswith(("COM!early.extensibility.", "COM!early.ribbon.")):
                require(entry["thread"] == opening["thread"], "Ribbon callback left the owning STA")
                if name.startswith("COM!early.extensibility."):
                    require(entry["object"] in objects, "Extensibility callback used an unbound object")
                else:
                    require(any(q["entry"] == "COM!QueryInterface" and q.get("iid") == "000c0396-0000-0000-c000-000000000046"
                                and q["object"] in objects and leaves[q["call"]].get("object") == entry["object"]
                                and leaves[q["call"]]["seq"] < entry["seq"] for q in enters.values()), "Ribbon interface is not bound to an activated add-in")
                row = {key: entry[key] for key in ("entry", "mode", "custom", "ribbon_id") if key in entry}
                row.update({key: returned[key] for key in ("hresult", "value") if key in returned})
                (starts if entry["seq"] < closed else ends).append(row)
            if name == "COM!Invoke" and opening["seq"] < entry["seq"] < leaves[opening["call"]]["seq"]:
                member = entry.get("member")
                if member in ("AutomationSecurity", "COMAddIns", "Update", "Item", "Connect"):
                    arguments = json.loads(json.dumps(entry["rgvarg"]))
                    if member == "Item":
                        require(len(arguments) == 1 and arguments[0]["vt"] == 8 and re.fullmatch(r"Dna_[a-f0-9]{32}_0", arguments[0]["value"]), "Unqualified private ribbon ProgID")
                        arguments[0]["value"] = "<private-progid>"
                    value = returned.get("result")
                    if isinstance(value, dict) and value.get("vt") == 9:
                        value = {"vt": 9, "value": "<object>"}
                    properties.append({"member": member, "flags": entry["flags"], "arguments": arguments,
                                       "hresult": returned["hresult"], "result": value})
        startup.append(starts); teardown.append(ends); property_calls.append(properties)
        traffic.append(dict(Counter(e["entry"] for e in enters.values() if e["entry"].startswith("COM!") and e["seq"] < closed)))
        bindings.append({"activated_objects": sorted(objects), "factory_calls": factories})
    require(startup[0] == startup[1], "Ribbon lifecycle arguments, HRESULTs or XML differ")
    require(property_calls[0] == property_calls[1] and len(property_calls[0]) == 7, "Ribbon COM helper property traffic differs")
    ui = [r for r in startup[0] if r["entry"] == "COM!early.ribbon.GetCustomUI"]
    require(len(ui) == 1 and ui[0]["ribbon_id"] == "Microsoft.Excel.Workbook" and ui[0]["hresult"] == 0 and ui[0]["value"], "Ribbon XML callback was not exercised once")
    require({r["entry"].rsplit(".", 1)[1] for r in startup[0]} == {"OnConnection", "OnAddInsUpdate", "OnDisconnection", "GetCustomUI"}, "Ribbon lifecycle coverage is incomplete")
    for directory in (host, host_control):
        output = json.loads((directory / "host.stdout").read_text())
        require(len(output["ribbons"]) == 1 and output["ribbons"][0][1] == ui[0]["value"], "Application did not retain the actual ribbon XML")
    if not result["status"].startswith("failed"):
        result["status"] = "verified bounded ribbon startup, helper properties and XML"
    result["ribbon"] = {"startup_callbacks_each_host": len(startup[0]), "startup": startup[0],
                        "helper_properties_each_host": len(property_calls[0]), "helper_properties": property_calls[0],
                        "teardown_callbacks_excel_and_host": teardown, "activation_bindings": bindings,
                        "all_com_entry_counts_excel_and_host": traffic,
                        "scope": "Same dummy/real startup callbacks and exact GetCustomUI XML on the owner STA. Raw duplicate CCW body entries remain. Remove+Close and UNREGISTER teardown differ and remain separately retained. Other COM metadata/identity queries, button actions and host UI painting are not qualified by this comparison."}
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("traced", type=Path)
    parser.add_argument("control", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--host", type=Path)
    parser.add_argument("--host-control", type=Path)
    parser.add_argument("--x2-cases", type=Path)
    parser.add_argument("--x3-cases", type=Path)
    parser.add_argument("--xloil-reload-cases", type=Path)
    parser.add_argument("--x4-cases", type=Path)
    parser.add_argument("--ribbon-cases", type=Path)
    parser.add_argument("--rtd-cases", type=Path)
    parser.add_argument("--workbook-pool-cases", type=Path)
    parser.add_argument("--permission-cases", type=Path)
    parser.add_argument("--permission-host", type=Path)
    parser.add_argument("--boundary-cases", type=Path)
    parser.add_argument("--boundary-host", type=Path, help="Host group directories of a run-batch.ps1 boundary batch; the first positional is the Excel run")
    parser.add_argument("--boundary-repeat", type=Path, help="A second Excel run of the same boundary cases (Excel-to-Excel control)")
    parser.add_argument("--ordered-host", type=Path, help="An engine run (dna-calc-desktop --engine) of --boundary-cases, compared by position; the first positional is the Excel run")
    parser.add_argument("--source-publication", type=Path)
    parser.add_argument("--excel-session", type=Path, help="Retained session ledger for native X2/X3 and extended RTD ownership checks")
    parser.add_argument("--identity", action="store_true")
    parser.add_argument("--pool", action="store_true")
    args = parser.parse_args()
    excel_session_directory = args.excel_session
    failed = False
    try:
        if args.ordered_host:
            result = verify_ordered(args.traced, args.ordered_host, args.boundary_cases, args.boundary_repeat)
        elif args.boundary_host:
            result = verify_boundary(args.traced, args.boundary_host, args.boundary_cases, args.boundary_repeat)
        elif args.permission_host:
            result = verify_permissions(args.traced, args.control, args.permission_host, args.permission_cases, args.source_publication)
        elif args.identity or args.pool:
            result = verify_x2_controls(args.traced, args.control, args.identity)
        elif args.host:
            parser.error("--host requires --host-control") if args.host_control is None else None
            result = (verify_xloil_reload(args.traced, args.control, args.host, args.host_control, args.xloil_reload_cases, args.source_publication) if args.xloil_reload_cases else
                      verify_ribbon(args.traced, args.control, args.host, args.host_control, args.ribbon_cases, args.source_publication) if args.ribbon_cases else
                      verify_workbook_pool(args.traced, args.control, args.host, args.host_control, args.workbook_pool_cases, args.source_publication) if args.workbook_pool_cases else
                      verify_rtd(args.traced, args.control, args.host, args.host_control, args.rtd_cases, args.source_publication) if args.rtd_cases else
                      verify_x2(args.traced, args.control, args.host, args.host_control, args.x4_cases or args.x3_cases or args.x2_cases, args.source_publication, x3=bool(args.x3_cases or args.x4_cases), x4=bool(args.x4_cases))
                      if any((args.x2_cases, args.x3_cases, args.x4_cases, args.ribbon_cases, args.rtd_cases, args.workbook_pool_cases, args.xloil_reload_cases)) else verify_x1(args.traced, args.control, args.host, args.host_control))
        else:
            result = verify(args.traced, args.control)
    except Exception as error:
        failed = True
        result = {"status": "failed verification", "error": str(error), "error_type": type(error).__name__}
        trace = error.__traceback__
        while trace and trace.tb_frame.f_code.co_name not in ("verify_x2", "verify_rtd", "verify_permissions", "verify_workbook_pool"):
            trace = trace.tb_next
        if trace:
            data = trace.tb_frame.f_locals
            if "changes" in data:
                changes = data["changes"]
                differences = [i for i, pair in enumerate(zip_longest(*(changes + [[]] * (2 - len(changes))))) if pair[0] != pair[1]]
                result.update({"formula_cases_checked": len(data.get("cases", [])), "native_invocations_checked": data.get("call_comparisons", []), "sheet_changes_excel_and_host": changes, "udf_early_com_excel_and_host": data.get("nested", []), "sheet_change_differences": differences, "scope": "Strict X4 comparison remains failed; matching earlier checks do not convert the differing event calls into passes."})
            elif trace.tb_frame.f_code.co_name == "verify_x2":
                result.update({"formula_cases_checked": len(data.get("cases", [])),
                               "native_function_under_comparison": data.get("name"),
                               "native_records_excel_and_host": data.get("records", []),
                               "native_invocations_checked": data.get("call_comparisons", {}),
                               "scope": "Strict native comparison remains failed. Retained callback operands are diagnostics, not additional passes."})
                try:
                    result["native_async_callback_operands_excel_and_host"] = [
                        [row for row in framework_callbacks(capture) if row["function"] == 16400]
                        for capture in data.get("traces", [])]
                except Exception as diagnostic_error:
                    result["callback_diagnostic_error"] = str(diagnostic_error)
            elif trace.tb_frame.f_code.co_name == "verify_workbook_pool":
                result.update({"formula_outcomes":data.get("outcomes",[]),"overlap_excel_and_host":data.get("overlaps",[]),"calculation_phase_calls_excel_and_host":data.get("worker_calls",[]),"native_invocations_excel_and_host":data.get("records",[]),"native_entry_counts_excel_and_host":data.get("entry_counts",[]),"ownership_excel_and_host":data.get("ownership",[]),"source_bindings_checked":data.get("source_bindings_checked",0),"compatibility_failures":data.get("failures",[]),"scope":"Strict full-profile failure; overlap and owning-activation evidence do not convert extra entry-time native calls or differing outcomes into passes."})
            elif trace.tb_frame.f_code.co_name == "verify_rtd":
                pairs = data.get("protocol_pairs", [])
                result.update({"host_results_checked": data.get("outputs", []), "source_bindings_checked": data.get("source_bindings_checked", 0), "client_protocols_excel_and_host": data.get("protocols", []), "raw_client_protocols_excel_and_host": data.get("raw_protocols", []), "client_protocol_comparison": pairs, "client_calls_matched": sum(r["matched"] for r in pairs), "client_calls_failed": sum(not r["matched"] for r in pairs), "client_calls_by_method": data.get("method_pairs", []), "server_bindings": data.get("server_bindings", []), "native_invocations_excel_and_host": data.get("native", []), "native_ownership_excel_and_host": data.get("ownership", []), "internal_server_bodies": data.get("internals", []), "scope": "Failed RTD comparison; successful value/provenance checks do not turn differing or incomplete traffic into passes."})
    if rewritten_source_bindings:
        result["rewritten_source_bindings"] = sorted(rewritten_source_bindings.values(), key=lambda x: (x["path"], x["recorded_revision"]))
    if session_ownership_bindings:
        result["excel_session_ownership"] = session_ownership_bindings
    failed = failed or result["status"].startswith("failed")
    rendered = json.dumps(result, indent=2) + "\n"
    if args.out:
        args.out.write_text(rendered, encoding="utf-8")
    print(rendered)
    if failed:
        raise SystemExit(1)
