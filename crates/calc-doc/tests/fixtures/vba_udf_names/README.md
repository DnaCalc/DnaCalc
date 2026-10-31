# VBA UDF name persistence witnesses (dc-snz.7)

Excel 16.0 build 20527, Windows 64-bit, 2026-10-31. These small projects, functions and scripts
were authored for this investigation and are MIT licensed with the repository. No external
workbook, legacy VBA engine source or third-party macro code is embedded. No merged cells.

The findings and specification links are in
[VBA_UDF_NAMES_IN_FILES.md](../../../../../docs/plan/VBA_UDF_NAMES_IN_FILES.md).
These are file-persistence witnesses, not an oracle value bank or a second calculation route.
File caches are recorded explicitly; only the COM recalculation observations establish execution.

Core packages: `local.xlsm`/`local.xlsb` (33 accepted formula entries, three rejected entries),
`stripped.xlsx`, `provider.xlsm`, `probe.xlam`, `consumer.xlsx`/`consumer.xlsb` (eight calls).
The small variants record disabled macros, renaming/case, XML-to-binary conversion, relocation,
function-help metadata and a UDF named LAMBDA. `dna-*` packages are actual calc-doc outputs.
Exported `.bas` files retain Excel's code-page bytes and CRLF; `.gitattributes` prevents normalization.
JSON/XML text is UTF-8.

Reproduce from this directory with Windows Python + pywin32, and Cargo for the product probe:

```powershell
python measure.py
python extra-cases.py
python metadata-and-identifiers.py
python relocate.py
# Optional interactive observation: python relocate.py --prompt
powershell -NoProfile -ExecutionPolicy Bypass -File measure-dnacalc.ps1
python reopen-product.py
python reopen-paths.py
python inspect.py
```

The product driver reuses `tests/cdoc_m1_1_support`, invokes the real document/compiler APIs,
and puts its temporary Cargo manifest and full output set in `target/udfnames/`.
Do not run it concurrently with another Cargo command using this clone's target directory.
`file-forms.json` dumps exact measured XML and binary records with package hashes; offsets refer
to uncompressed parts, and `record_hex` includes the record header. `inspect.py` only extracts evidence.

Each COM script starts its own instance, records its PID, retries busy calls and quits that
instance. The add-in is opened rather than installed. `relocate.py` temporarily moves only the
generated `probe.xlam` and restores it in `finally`. Disabled-macro opening uses a temporary
per-open AutomationSecurity policy, restored afterwards. No Trust Center/registry changes.
The link-prompt wording and selected Update action were also observed through Computer Use;
no subsequent file-repair prompt appeared. Other interactive configurations remain unmeasured.

The dc-snz.8/dc-snz.12 fixes are checked by `tests/vba_udf_names.rs`. To produce fresh
outputs without replacing the investigation's original packages or observations:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File measure-dnacalc.ps1 -NoRecord -OutDir ../../../../../target/udfnames/p1-final
python reopen-product.py --product ../../../../../target/udfnames/p1-final --out cdoc-p1-reopen.json --paths
```

`cdoc-path-diagnostic.json` records the intermediate failure with alternate URLs alone and
the successful controls restoring the workbook folder (`BrtAbsPath15`, record 2071), removing
alternate URLs after restoring that folder, and selecting the absolute primary relationship.
`--path-variants` reproduces those controls from an output lacking the workbook folder.
`cdoc-p1-reopen.json` records the corrected product saves and asserts the two input formats'
5005 results, both output folders' provider results, and rejection of the wrong add-in location.

`--binding-controls` on the same reopen script creates `binding-arguments.xlsx` and records
`qualified-reference-arguments` in the selected JSON output. Excel calculates both external
workbook/module calls with consumer-cell arguments as 2002 (`cdoc-binding-controls.json`).
The compiler tests use identity adapters to check name/provider binding and argument reads;
they neither execute VBA nor treat file caches as calculated results.
