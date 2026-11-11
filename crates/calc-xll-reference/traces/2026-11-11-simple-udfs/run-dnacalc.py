"""Runs a case file through DNA Calc's desktop executable, one process per case, and compares each
result with Excel's answer recorded in the case file (the `value` the Excel run wrote).

    python run-dnacalc.py --exe dna-calc-desktop.exe --addin <file.xlam|file.xll> --cases <cases.json> --out <result.json>
    python run-dnacalc.py --exe dna-calc-desktop.exe --workbook <file.xlsm> --cases <cases.json> --excel <reopen.json> --out <result.json>

The route is the executable's own: `dna-calc-desktop --engine --xll <add-in> --formula <text>`
(CalcHost's checked route with the add-in set of calc-addin-host, the set app mode loads with the
same --xll option); with --workbook, `--engine --workbook <file.xlsm> --formula <text>`
(the macro workbook opened and its own VBA project compiled through calc-host-vba's open-time
route, as the page's worker opens one; the formula entered in it). Excel's answers for a workbook come
from `--excel`, the `reopen.json` of `vba-job.ps1 -Action reopen` (Excel calculating the same
formula inside the same .xlsm), matched by formula. One process per case because the engine harness stops at its first refusal.
A case is matched (same value as Excel), mismatched (a different calculated value) or refused
(the executable calculated nothing: a typed refusal or a failed run). Pass = matched; fail =
mismatched + refused + missing (Excel gave no answer), over all cases.
"""
import argparse
import json
import math
import subprocess


def same(expected, actual):
    if isinstance(expected, bool) or isinstance(actual, bool):
        return type(expected) is type(actual) and expected == actual
    if isinstance(expected, (int, float)) and isinstance(actual, (int, float)):
        return expected == actual or math.isclose(expected, actual, rel_tol=1e-15, abs_tol=0.0)
    if expected is None:
        return actual is None or actual == 0 or actual == ""
    return expected == actual


MISSING = "<no Excel answer>"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", required=True)
    parser.add_argument("--addin")
    parser.add_argument("--workbook")
    parser.add_argument("--excel")
    parser.add_argument("--cases", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    with open(args.cases, encoding="utf-8") as f:
        cases = json.load(f)
    if bool(args.addin) == bool(args.workbook):
        parser.error("give one of --addin and --workbook")
    if args.excel:
        names = {-2146826288: "#NULL!", -2146826281: "#DIV/0!", -2146826273: "#VALUE!", -2146826265: "#REF!",
                 -2146826259: "#NAME?", -2146826252: "#NUM!", -2146826246: "#N/A"}
        with open(args.excel, encoding="utf-8-sig") as f:
            reopened = json.load(f)["rows"]
        answers = {}
        for row in reopened:
            excel = row.get("excel") or {}
            value = {"error": names.get(excel["error_code"], excel["error_code"])} if "error_code" in excel else excel.get("value")
            answers[row.get("formula")] = float(value) if isinstance(value, int) and not isinstance(value, bool) else value
        cases = [dict(case, value=answers.get(case["formula"], MISSING)) for case in cases]
    route = ["--xll", args.addin] if args.addin else ["--workbook", args.workbook]
    rows, counts = [], {"matched": 0, "mismatched": 0, "refused": 0, "missing": 0}
    for case in cases:
        if case["value"] == MISSING:
            counts["missing"] += 1
            rows.append({"formula": case["formula"], "outcome": "missing"})
            print(f'missing    {case["formula"]} (no Excel answer)')
            continue
        run = subprocess.run(
            [args.exe, "--engine", *route, "--formula", case["formula"]],
            capture_output=True, timeout=120,
        )
        stdout = run.stdout.decode("utf-8", "replace").strip()
        stderr = run.stderr.decode("utf-8", "replace").strip()
        row = {"formula": case["formula"], "excel": case["value"], "exit": run.returncode}
        try:
            result = json.loads(stdout.splitlines()[-1])["results"][0]
            row["dnacalc"] = result.get("value")
            row["outcome"] = "matched" if same(case["value"], row["dnacalc"]) else "mismatched"
        except (IndexError, KeyError, ValueError):
            row["outcome"] = "refused"
            row["why"] = (stderr or stdout)[-400:]
        counts[row["outcome"]] += 1
        rows.append(row)
        print(f'{row["outcome"]:<10} {case["formula"]:<40} excel={json.dumps(case["value"], ensure_ascii=False)} dnacalc={json.dumps(row.get("dnacalc"), ensure_ascii=False)} {row.get("why", "")[:160]}')
    summary = {"exe": args.exe, "addin": args.addin, "workbook": args.workbook, "excel": args.excel, "cases_file": args.cases, "count": len(cases), **counts, "rows": rows}
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=1)
    print(f'DNACALC {args.addin or args.workbook}: matched {counts["matched"]}/{len(cases)} mismatched {counts["mismatched"]} refused {counts["refused"]} missing {counts["missing"]}')


if __name__ == "__main__":
    main()
