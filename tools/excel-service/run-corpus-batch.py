"""Run the Corpus V2 batch through the owned Excel session client, or audit it offline."""

import argparse
import csv
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys


SERVICE = Path(__file__).resolve().parent
ROOT = SERVICE.parents[1]
SESSION = SERVICE.parent / "excel-session"
CORPUS_BUCKET = "r2:dnacalc-corpus/"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_manifest(path):
    with path.open(encoding="utf-8-sig", newline="") as source:
        return list(csv.DictReader(source, delimiter="\t"))


def load_job(job_path, cp1_manifest):
    job = json.loads(job_path.read_text(encoding="utf-8"))
    require(job["format"] == "dnacalc-excel-corpus-batch/1", "Unknown corpus batch format")
    for field in ("timeout_per_workbook", "timeout_vba_batch"):
        require(type(job[field]) is int and job[field] > 0, f"Invalid {field}")

    authored = job["authored"]
    manifest = ROOT / authored["manifest"]
    manifest_bytes = manifest.read_bytes().replace(b"\r\n", b"\n")
    require(hashlib.sha256(manifest_bytes).hexdigest() == authored["sha256"],
            "Authored capture manifest changed")
    rows = read_manifest(manifest)
    require(len({row["id"] for row in rows}) == len(rows), "Duplicate authored case ID")
    files = {"formula": [], "vba": []}
    for row in rows:
        case, kind = row["id"], row["kind"]
        require(re.fullmatch(r"[a-z0-9-]+", case), f"Invalid case ID: {case}")
        paths = {"formula": f"formulas/{case}/example.xlsx", "vba": f"vba/{case}/module.bas"}
        require(row["path"] == paths.get(kind), f"Unexpected input for {case}")
        path = manifest.parent / row["path"]
        metadata = json.loads(path.with_name("manifest.json").read_text(encoding="utf-8"))
        capture = row["capture"].split(";")
        require((metadata["id"], metadata["kind"], metadata["capture"]) == (case, kind, capture),
                f"Capture manifest differs from case metadata: {case}")
        if kind == "formula":
            anchors = {f"Sheet1!{cell}" for cell in metadata["formulas"]}
            require({cell.removesuffix("#") for cell in capture} == anchors,
                    f"Formula targets differ from authored cells: {case}")
        else:
            require(capture == [metadata["entry_point"]] == ["DcExample"],
                    f"Unexpected VBA entry point: {case}")
        files[kind].append((row, path, sha256(path)))

    for kind, cases in files.items():
        hashes = "".join(f"{row['path']}\t{digest}\n" for row, _, digest in cases)
        require(len(cases) == authored[kind]["count"] == len({x[2] for x in cases}),
                f"Unexpected {kind} count or duplicate content")
        require(hashlib.sha256(hashes.encode()).hexdigest() == authored[kind]["files_sha256"],
                f"Authored {kind} inputs changed; review the bytes and regenerate the job")

    cp1 = job["cp1"]
    require(sha256(cp1_manifest) == cp1["sha256"], "cp1 selection manifest changed")
    selected = read_manifest(cp1_manifest)
    require(len(selected) == cp1["count"] == len({row["sha256"] for row in selected}),
            "Unexpected cp1 count or duplicate hash")
    require(len({row["r2_key"] for row in selected}) == len(selected), "Duplicate cp1 object key")
    for rank, row in enumerate(selected, start=1):
        key = PurePosixPath(row["r2_key"])
        require(int(row["rank"]) == rank and re.fullmatch(r"[0-9a-f]{64}", row["sha256"]),
                f"Invalid cp1 rank or hash: {rank}")
        require(key.parts[:2] == ("sources", row["set"]) and ".." not in key.parts
                and "\\" not in row["r2_key"] and key.suffix.lower() == ".xlsx",
                f"Invalid source key: cp1 rank {rank}")
        require(0 < int(row["bytes"]) <= 2 * 1024 * 1024 and 0 < int(row["formula_cells"]) <= 30000
                and all(row[field] for field in ("source_url", "licence", "licence_evidence_url")),
                f"Missing cp1 inventory or provenance: rank {rank}")
    for field in ("bytes", "formula_cells"):
        require(sum(int(row[field]) for row in selected) == cp1[field], f"cp1 {field} total differs")

    targets = [target for row, _, _ in files["formula"] for target in row["capture"].split(";")]
    print(f"Validated {len(files['formula'])} authored formula workbooks "
          f"({len(targets)} capture targets, {sum(t.endswith('#') for t in targets)} spill reads), "
          f"{len(files['vba'])} triage-reviewed VBA drivers, {len(selected)} cp1 workbooks "
          f"({cp1['bytes']:,} bytes, {cp1['formula_cells']:,} inventoried formula cells).", flush=True)
    return job, files, selected


def run(*args, env=None, check=True):
    return subprocess.run(args, cwd=ROOT, env=env, check=check).returncode


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def stage(job, files, selected, out, rclone):
    for kind in ("formula", "vba", "cp1"):
        (out / kind / "inputs").mkdir(parents=True)
    for row, path, digest in files["formula"]:
        target = out / "formula" / "inputs" / row["id"] / path.name
        target.parent.mkdir()
        shutil.copyfile(path, target)
        require(sha256(target) == digest, f"Authored package changed: {row['id']}")
    drivers = []
    for row, path, digest in files["vba"]:
        target = out / "vba" / "inputs" / f"{row['id']}.bas"
        shutil.copyfile(path, target)
        require(sha256(target) == digest, f"Authored module changed: {row['id']}")
        drivers.append({"id": row["id"], "path": str(target), "sha256": digest})
    for row in selected:
        target = out / "cp1" / "inputs" / row["sha256"] / PurePosixPath(row["r2_key"]).name
        target.parent.mkdir()
        run(*rclone, "copyto", "--checksum", CORPUS_BUCKET + row["r2_key"], str(target))
        require(target.stat().st_size == int(row["bytes"]) and sha256(target) == row["sha256"],
                f"cp1 source mismatch: rank {row['rank']}")
    write_json(out / "vba" / "request.json", {"drivers": drivers})
    shutil.copyfile(ROOT / job["authored"]["manifest"], out / "authored-manifest.tsv")


def acquire(job_path, out):
    job = json.loads(job_path.read_text(encoding="utf-8"))
    env = os.environ.copy()
    env["DNACALC_EXCEL_PACKET"] = str(out)
    statuses = {}
    submit = ("pwsh", "-NoProfile", "-NonInteractive", "-File", str(SESSION / "submit.ps1"))
    for kind in ("formula", "cp1", "vba"):
        folder = out / kind
        env["DNACALC_EXCEL_SESSION"] = str(folder / "session")
        try:
            if kind == "vba":
                # submit.ps1 takes a JSON parameter object for one batched owned-session job.
                parameters = json.dumps({"Request": str(folder / "request.json"),
                                         "Result": str(folder / "result.json")})
                statuses[kind] = run(*submit, "-Script", str(SERVICE / "corpus-vba-job.ps1"),
                                     "-Parameters", parameters, "-Environment", "corpus-vba-example",
                                     "-TimeoutSeconds", str(job["timeout_vba_batch"]),
                                     "-FreshRequired", "-NoSaveBaseline", env=env, check=False)
            else:
                corpus = ("cargo", "xtask", "corpus", str(folder / "inputs"), "--recursive",
                          "--cache", str(folder / "captures"),
                          "--timeout", str(job["timeout_per_workbook"]), "--resume")
                statuses[kind + "_acquisition"] = run(
                    *corpus, "--capture-only", "--out", str(folder / "acquisition"), env=env, check=False)
                statuses[kind + "_comparison"] = run(
                    *corpus, "--no-capture", "--out", str(folder / "comparison"), env=env, check=False)
        finally:
            if (folder / "session" / "host.json").exists():
                statuses[kind + "_shutdown"] = run(*submit, "-Stop", env=env, check=False)
            write_json(out / "command-status.json", statuses)
    return int(any(statuses.values()))


def ps_quote(value):
    return "'" + str(value).replace("'", "''") + "'"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("job", type=Path)
    parser.add_argument("--cp1-manifest", type=Path, help="local copy of the frozen cp1 TSV")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--validate-only", action="store_true")
    mode.add_argument("--out", type=Path, help="new output directory in the approved Windows VM")
    mode.add_argument("--acquire-staged", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()

    if args.validate_only:
        if args.cp1_manifest is None:
            parser.error("--validate-only requires --cp1-manifest")
        load_job(args.job, args.cp1_manifest)
        return 0

    require(os.name == "nt", "Acquisition requires the approved Windows Excel VM")
    if args.acquire_staged:
        return acquire(args.job, args.acquire_staged)

    out = args.out.resolve()
    require(not out.is_relative_to(ROOT), "Use an output directory outside the checkout")
    out.mkdir(parents=True, exist_ok=False)
    config = Path(os.environ["APPDATA"]) / "dnacalc" / "rclone.conf"
    rclone = ("rclone", "--config", str(config))
    evidence = out / "cp1-manifest.tsv"
    if args.cp1_manifest:
        shutil.copyfile(args.cp1_manifest, evidence)
    else:
        remote = json.loads(args.job.read_text(encoding="utf-8"))["cp1"]["manifest"]
        require(remote.startswith(CORPUS_BUCKET), "Unexpected cp1 manifest bucket")
        run(*rclone, "copyto", "--checksum", remote, str(evidence))
    job, files, selected = load_job(args.job, evidence)
    stage(job, files, selected, out, rclone)
    job_copy = out / "job.json"
    write_json(job_copy, job)

    command = "& " + " ".join(map(ps_quote, (
        sys.executable, Path(__file__).resolve(), job_copy, "--acquire-staged", out)))
    guard = out / "desktop-guard.json"
    status = run("pwsh", "-NoProfile", "-NonInteractive", "-File", str(SESSION / "on-private-desktop.ps1"),
                 "-Command", command, "-Log", str(out / "acquisition.log"), "-GuardOut", str(guard),
                 "-SessionRoot", str(out), "-DesktopName", f"DnaCalcCorpusV2-{os.getpid()}", check=False)
    require(guard.is_file(), "Private desktop guard receipt is missing")
    require(json.loads(guard.read_text(encoding="utf-8"))["windows_on_user_desktop"] == 0,
            "An owned Excel window appeared on the user desktop")
    return status


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
