# Corpus VBA census

Run `census.py` with the host's `venv-ole` Python. It hashes workbooks under
`~/corpora/`, detects VBA and XLM statically, and parses each VBA project's
decompressed `dir` stream with oletools. It does not launch Office, evaluate
workbook formulas, or execute VBA.

The R2 `windows/corpus/` workbook listing is supplied with `--listing`. R2
`.xlsm` and `.xlsb` files are staged locally before the run; `.xlsx` and `.xltx`
objects are counted from the listing and intentionally remain undownloaded.
Their SHA-256 identities therefore cannot be deduplicated against local files.
The summary reports those listing-only objects separately.

Outputs are `projects.jsonl` (one row per workbook-project), `references.tsv`,
`coverage.json`, and `summary.json`. Reference fields are taken from the OVBA
records; records the parser cannot read are labelled unreadable or truncated.

The `dir` stream is read record by record (MS-OVBA 2.3.4.2) with olefile and the
oletools decompressor only. Control references (REFERENCECONTROL) are keyed by the
GUID of their original libid: the extended libid names the per-machine `.exd`
cache, a fresh GUID per machine, which would split MSForms into hundreds of keys.
Files that fail as OLE and are 1,040,000-1,048,576 bytes are labelled
`file-cut-at-1MiB` (the FUSE crawl stored at most 1 MiB per file; dc-afua).

Coverage: a workbook is covered by a library set only when every reference of its
readable project is in the set; unreadable projects are never covered. The exact
search for k = 1..5 is a depth-first branch and bound over the distinct reference
sets (an optimal k-set is, up to padding, a union of the sets it covers), reported
with and without the four always-present libraries (VBA, Excel, stdole, Office;
VBA and Excel are implicit and never stored in the dir stream). `summary.json` is
written before the coverage search. `--summaries-from-existing` rebuilds the
references and coverage from an existing `projects.jsonl`.

## Host-reach footprint (dc-54aki)

`footprint.py` reads module source (olefile and its own MS-OVBA decompressor; no oletools
needed) and records, per procedure, the host-reach constructs it uses: file statements,
`Environ`, `Shell`, `CreateObject` ProgIDs, early-bound external classes, called `Declare`
entry points, object-model members that reach the host, and the shapes of path literals. It
evaluates `#If` with the Windows dialect constants, then closes each procedure's set over the
project's call graph. It writes `procedures.jsonl` with construct names only, never source text.

`footprint_report.py` holds the verdict tables (feature, COM class and Declare group, one
verdict per host) and joins the scans with `dnacalc run --all --fixtures --macro-level guarded
--json` outputs of the triage-clean set. It writes the TSVs in
`docs/design/portable-execution/`. The triage-suspect set may be scanned; it is never run.

```
python3 -I footprint.py --list <paths> --root <dir> --out <scan> [--watch-members <names>] [--watch-vba <names>]
python3 -I footprint_report.py --clean <scan> [--suspect <scan>] [--runs <dir>] \
    [--calcvba-matrix <CalcVBA>/crates/calcvba-symbol/src/portability/matrix.rs] --out docs/design/portable-execution
```

This is a regex scan, the first measurement for docs/design/PORTABLE_EXECUTION.md. The product
check is CalcVBA's portability report over bound code (dc-8svfs.1).
