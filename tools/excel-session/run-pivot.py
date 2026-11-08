"""Runs pivot-job.ps1 cases through the owned Excel session host, one job per case.

  py -3 tools/excel-session/run-pivot.py --out DIR --session DIR CASE [CASE ...]

A CASE is `kind` or `kind:scenario` (kinds and scenarios are those of pivot-job.ps1). Each job gets
DIR/observations/<id>/ (request, script copy, console log, result.json, receipt copy) and
DIR/files/<id>/ (saved workbooks). A failed job is retained and stops the run; nothing is retried.
Run it under tools/excel-session/on-private-desktop.ps1 so Excel stays off the owner's desktop.
"""
import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=1800)
    parser.add_argument("--extra", default="{}", help="JSON merged into every request")
    parser.add_argument("cases", nargs="+")
    options = parser.parse_args()
    extra = json.loads(options.extra)
    status_all = []
    for case in options.cases:
        kind, _, scenario = case.partition(":")
        ident = case.replace(":", "-")
        directory = options.out / "observations" / ident
        files = options.out / "files" / ident
        if directory.exists():
            raise SystemExit(f"{directory} exists; choose a new --out or case id (evidence is never overwritten)")
        directory.mkdir(parents=True)
        files.mkdir(parents=True)
        request = {"id": ident, "kind": kind, "scenario": scenario or None, "directory": str(files), **extra}
        write(directory / "request.json", request)
        for name in ("pivot-job.ps1", "hidden-window.ps1", "pivot-m4-formulas.tsv"):
            shutil.copy2(HERE / name, directory / name)
        arguments = ["pwsh", "-NoProfile", "-File", str(HERE / "submit.ps1"), "-SessionDirectory", str(options.session),
                     "-Script", str(directory / "pivot-job.ps1"), "-Parameters",
                     json.dumps({"Request": str(directory / "request.json"), "Result": str(directory / "result.json")}),
                     "-Environment", "pivot-measurement", "-TimeoutSeconds", str(options.timeout)]
        with (directory / "console.log").open("w", encoding="utf-8") as log:
            code = subprocess.run(arguments, stdout=log, stderr=subprocess.STDOUT).returncode
        console = (directory / "console.log").read_text(encoding="utf-8-sig", errors="replace")
        receipts = [line.split(" receipt ", 1)[1] for line in console.splitlines()
                    if line.startswith("dnacalc-excel-session/1 receipt ")]
        if len(receipts) == 1:
            shutil.copytree(Path(receipts[0]).parent, directory / "session")
        result = None
        if (directory / "result.json").exists():
            result = json.loads((directory / "result.json").read_text(encoding="utf-8-sig"))
        status = {"id": ident, "exit": code, "result": result is not None,
                  "fatal": result.get("fatal") if result else "no result",
                  "errors": len(result.get("errors", [])) if result else None,
                  "hidden": result.get("visibility", {}).get("compliant") if result else None}
        write(directory / "status.json", status)
        print(json.dumps(status), flush=True)
        status_all.append(status)
        if code or not result or result.get("fatal"):
            print("job failed; stopping without retry", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
