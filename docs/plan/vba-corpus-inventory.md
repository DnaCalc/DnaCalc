# VBA-bearing files in the Touchstone corpora (2026-11-12)

Static inventory and triage of every spreadsheet file in the sets already in R2, plus two new sources
with VBA. Done on kode-fi (never executed anything, never opened a file in Excel). Counts only: no
workbook content, no code, no file names of public-corpus files are committed here. Per-file
inventory (path, size, SHA-256, kind, VBA/XLM flags) and triage (verdict, reasons) are in R2 under
`dnacalc-corpus/runs/kode-fi/20261112-vba-triage/` (`<set>.inv.jsonl`, `<set>.triage.jsonl`,
`summary.json`, `owner_review.jsonl`).

## Method

* Inventory (`scan.py`, python `olefile` + `zipfile`, content-based, not by extension): OOXML and
  .xlsb: a `vbaProject.bin` part; `xl/macrosheets/` for Excel 4.0 (XLM) macro sheets. OLE2 .xls: a
  `_VBA_PROJECT_CUR` or `VBA` storage; XLM from BIFF BOUNDSHEET records of type 1. OLE2 files whose
  structure cannot be read (truncated) are checked by a raw search for the UTF-16 `_VBA_PROJECT`
  directory entry and counted separately.
* Triage (`triage.py`, oletools 0.60.2 `olevba` for extraction, `pcodedmp` for the stomping check,
  plus our own pattern pass over the extracted source; reasons only are written, never code):
  * danger: process start (Shell, WScript.Shell, Shell.Application, ShellExecute, WinExec, WMI,
    CreateProcess); command-line tools (powershell, cmd /c, mshta, regsvr32, certutil, bitsadmin,
    rundll32, wmic, cscript); downloads (URLDownloadToFile, XMLHTTP, WinHttp, ADODB.Stream,
    InternetOpen/ReadFile); obfuscation (chains of 8 or more Chr terms, StrReverse, quoted base64 or
    hex of 100+ bytes); Declare imports of dangerous APIs (VirtualAlloc, RtlMoveMemory,
    SetWindowsHookEx, CreateProcess, registry writes and so on); XLM EXEC/CALL/REGISTER; VBA stomping.
  * effect: Declare imports of other system libraries; file write or delete, FileSystemObject; registry
    access; Environ; SendKeys; CreateObject/GetObject of any other class (Dictionary, Excel.Application
    and the like are exempt); mail send; self-modifying VBE code; URL literals.
  * info: auto-run entry points (Workbook_Open, Auto_Open and so on), saveas, OnTime.
  * Public corpora: suspect only on a danger sign. Harvested sets (GitHub): any danger or effect.
  * Not evidence of malice by itself: Windows-API Declares and ShellExecute for opening a URL are normal
    in 1990s and 2000s office macros.

## Counts per set

"VBA files" is by file path (duplicates by content are counted once in the unique figure where it
differs). "Readable" excludes OLE2 files truncated by the crawler (below).

| Set | Files | VBA files | Unique | XLM sheets | Triaged | Clean | Suspect (other danger) | Suspect (stomping only) | Unanalysed |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| FUSE (readable) | 249,376 | 2,194 | 2,194 | 6 | 2,199 | 2,178 | 8 | 13 | 0 |
| FUSE (truncated, raw scan) | (9,534 truncated) | 3,156 | 3,156 | - | not analysable | - | - | - | 3,156 |
| EUSES | 5,598 | 360 | 334 | 2 | 362 | 346 | 6 | 10 | 0 |
| Enron | 15,929 | 0 | 0 | 0 | - | - | - | - | - |
| Apache POI | 821 | 61 | 60 | 3 | 64 | 62 | 1 | 1 | 0 |
| LibreOffice | 1,731 | 68 | 68 | 0 | 68 | 67 | 0 | 0 | 1 (file open error) |
| calamine | 160 | 7 | 7 | 0 | 7 | 7 | 0 | 0 | 0 |
| ClosedXML | 385 | 2 | 2 | 0 | 2 | 2 | 0 | 0 | 0 |
| XlsxWriter | 1,005 | 5 | 5 | 0 | 5 | 5 | 0 | 0 | 0 |
| ExcelJS, Excelize | 46 | 0 | 0 | 0 | - | - | - | - | - |
| Zenodo VBA search (new) | 137 | 114 | 114 | 0 | 114 | 82 | 10 | 22 | 0 |
| GitHub permissive repos (new, harvested rule) | 2,858 | 755 | 741 | 1 | 756 | 270 | 322 | 164 | 0 |
| Sheetpedia XLSX (2026-11-13; no VBA/XLM found) | 211,323 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| BigSpill Excel Lambda Library (2026-11-13; no VBA/XLM found) | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Replicator Dynamics (2026-11-13; no VBA/XLM found) | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| GitHub VBA workbooks (2026-11-14) | 2 | 2 | 2 | 0 | 2 | 1 | 1 (danger and effect) | 0 | 0 |
| GitHub macro-tool workbook (2026-11-14) | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 0 | 0 |
| GitHub feature examples (2026-11-14; no VBA/XLM found) | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

By kind: FUSE and EUSES VBA files are all binary .xls (OLE2); Zenodo 110 .xlsm plus 4 .xlsb; GitHub
683 OOXML, 37 .xlsb, 35 .xls. Enron holds no VBA at all (the archive is the .xlsx conversion).
Totals: 2,194 + 360 + 61 + 68 + 7 + 2 + 5 = 2,697 readable VBA files in the existing public sets (about
3,156 more in FUSE that cannot be read), and 869 in the two new sources.

For the three spreadsheet-only sources added on 2026-11-13, the zero VBA/XLM and triage counts mean
the static inventory found no macro projects or macro sheets to triage. Formula and feature counts
are recorded in [`corpus-sources.md`](corpus-sources.md).

### Additional licensed GitHub sets (2026-11-14)

The three licensed GitHub macro-enabled sets add four unique VBA workbooks. `github-vba-20261114` has two `.xlsm` workbooks: one triage-clean and one suspect (danger and effect signals). `github-vba-tools-20261114` has one `.xlsm`, triage-clean and with no formulas. `github-vba-licensed-extra-20261114` has one `.xlsm`, suspect for a VBA-stomping signal; it has 2,115 formula cells, two tables, seven PivotTables and one slicer. A separate feature set has three `.xlsx` files, 794 formula cells, no VBA or XLM, two files with slicers and three with PivotTables. Its capture candidates are in `~/codex-runs/harv2/capture-batches/`; they are formula bearing, macro-free files no larger than 20 MiB. All these R2 set copies passed one-way checks.

These are harvested GitHub sets, so either a danger or effect sign marks a workbook suspect. The
two vbaXray workbooks were omitted because their SHA-256 hashes were already present in the earlier
GitHub corpus. Workbook paths stay in the local manifests and R2 inventory; no workbook code is
committed.

Observations:

* FUSE: 9,534 files (3.8 percent) are OLE2 files cut at 1 MiB (the Common Crawl truncation limit);
  3,156 of them still carry the VBA directory entry. They cannot be opened by Excel or olevba; keep
  them out of any capture batch.
* Stomping flag: 13 of 21 FUSE suspects, 10 of 16 EUSES suspects, 22 of 32 Zenodo suspects carry only
  the P-code versus source mismatch. oletools documents this check as heuristic; on 1990s and 2000s
  OLE2 files and on files saved by other tools it is very likely a false positive. Treated as suspect
  as the rules say, reported separately above.
* Auto-run entry points: FUSE 395, EUSES 69, POI 13, LibreOffice 28, Zenodo 58, GitHub 317.
* Top danger reasons among the "other danger" suspects: public sets: process start (ShellExecute or
  Shell to open a URL or a program), dangerous Declares (ShellExecute, RegCreateKey, wininet),
  download APIs (2 FUSE, 3 Zenodo), one Chr chain and one StrReverse in FUSE, one command-line tool
  in POI. GitHub: download API 115, process start 101, object classes (WScript, Shell) 84, dangerous
  Declares 70, command-line tools 34, WMI 14, StrReverse 10; 111 more are suspect on effects alone
  (file write or delete 49, other COM classes 43, URL literals 28, system Declares 20).

## Anything malicious

Nothing confirmed. Public corpora show no combination of auto-run, obfuscation and download or
execution, apart from the items in `owner_review.jsonl` (96 files, hashes and reasons only; 92 in the
GitHub set, 4 in Zenodo). The strongest are seven GitHub files (SHA-256 prefixes 416ddac625,
f7dbd18607, 1c9779fe93, da59bfb55b, 86d81ed068, 2f9aca64f4, de123b9945) that combine obfuscation
(Chr chain or StrReverse) with process start or download and auto-run, from repositories named as
macro tooling, a sample collection and a game; they look like developer tools or red-team samples, not
confirmed malware. Marked for the owner's decision. Reporting to maintainers needs the owner's yes.
Treat every file in the GitHub set as untrusted: never open in Excel with macros enabled.

## New sources

| Source | What | Size | Licence as stated | R2 |
|---|---|---:|---|---|
| Zenodo records found by search (97 records, 137 files; .xlsm 110+, .xlsb, .xls) | research workbooks with VBA tools: models, simulation front ends | 1,285,799,059 bytes (tar 1.2 GiB); all 137 md5 verified against Zenodo | per record: CC BY 4.0, CC0, MIT, public domain, CC BY-SA, GPL (record metadata) | `sources/zenodo-vba/20261112/` |
| GitHub, 1,343 repositories with a permissive licence from 1,395 search hits (442 contain spreadsheets) | workbooks and add-ins shipped with VBA projects and tests | 495,523,840 bytes (tar); 2,858 files, 2,648 unique hashes | repository licence as GitHub states it: MIT 2,577 files, Apache-2.0 236, CC0 22, Unlicense 11, BSD 9, ISC 2, 0BSD 1; no per-file check | `sources/github-vba/20261112/` |

Each prefix holds the tar, its SHA-256, `manifest.jsonl` (source URL or repo, path, size, SHA-256,
licence per file) and `SOURCE.json`. Never redistribute; private research copies.

Limits of the GitHub harvest: unauthenticated search returns at most 1,000 results per query, so the
set is the top repositories by stars per licence, not all (GitHub reports 2,791 MIT repositories
with language VBA alone). Repositories over 60 MB and forks were skipped (52). More would need a
token or a licensed GitHub archive, neither used. Zenodo only matches the word xlsm or VBA in
metadata, so dataset records that bury a workbook in a zip were not unpacked.

### Zenodo records not downloaded (11)

Licence not clearly open: CC BY-NC (3), CC BY-NC-ND (4), CC BY-NC-SA (1), PolyForm Noncommercial (1),
none stated (2). Hashes and titles are in
`SOURCE.json` under `not_downloaded`. Left out until the owner decides whether non-commercial
research use is acceptable.

### VBA tips sites checked (nothing downloaded)

Rule: download only what is clearly allowed. Sites checked by HTTP on 2026-11-12:

| Site | Finding | Action |
|---|---|---|
| contextures.com, chandoo.org, mrexcel.com, excelforum.com, thespreadsheetguru.com, excelmacromastery.com, xelplus.com | HTTP 403 to a plain client (bot protection); terms could not be read; no attempt to bypass | listed, not downloaded |
| vbaexpress.com (KB), cpearson.com | no response from kode-fi | listed |
| excel-easy.com | terms page only covers ads and cookies; "free" but no reuse or redistribution grant | not downloaded |
| rondebruin.nl, wellsr.com, analysistabs.com, excel-vba.com, exceltip.com, extendoffice.com, ablebits.com | no licence or reuse statement found on the pages read | not downloaded |

Next step if the owner wants these: write to the site owners for permission (needs the owner's yes), or
use their code snippets only as hand-typed test cases.

## Files and tools

Working copies stay on kode-fi under `~/corpora/` (sets, `fuse-vba/` holds the 5,350 or so FUSE VBA/XLM files by
hash, `new/` the two new sources); can be deleted. Scripts: `~/vbawork/scan.py`, `triage.py` on kode-fi
(not committed; they are about 400 lines and can be added to the repo on request).


The Mendeley CC BY 4.0 set `mendeley-vba-20261114` adds four unique `.xlsm` projects. Static extraction found 12 modules across the four readable projects; harvested-set triage marked all four clean. None has formula cells or XLM macro sheets. The four project rows are in the local census output.

The pinned MIT source `github-vba-licensed-extra-20261114` adds one unique `.xlsm` project (28 modules). It is readable and has no XLM sheet; harvested-set triage marked it suspect for VBA stomping. The four Mendeley projects plus this project extend the local census to 7,027 unique VBA workbooks (3,909 readable projects); the exact library-coverage results remain from the prior 7,022-project snapshot because recomputing them exceeded the host memory budget. The new workbook references stdole, Office and MSForms.
