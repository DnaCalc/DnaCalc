# Candidate public corpora for Excel parity (epic dc-ztvv)

Status 2026-11-11. This list extends the candidate list in bead dc-ebb (comment of 2026-10-27, items
1 to 8); it does not restart it. Sizes and licences are as each source's own record states them, read
from the Zenodo, figshare and GitHub APIs on 2026-11-11. For the 2026-11-11 list, "Downloaded" means fetched, hashed, copied to R2 `dnacalc-corpus/sources/<corpus>/20261111/` with a `SOURCE.json` licence record, and profiled with `cargo xtask classify`. The 2026-11-13 local sets below are retained on kode-fi for later copy to the Windows capture host; their manifests and profile counts are linked from `corpus-sources.md`. No workbook is committed.

## Downloaded (owner's yes, 2026-11-11)

| Corpus | Source (exact) | Archive size | Spreadsheet files | Licence as stated | What it adds |
|---|---|---:|---:|---|---|
| EUSES | https://zenodo.org/records/581673 (12 files, md5 verified) | 194.1 MB (11 zips + README) | 4,652 after unzip (4,608 OLE2 .xls) | CC BY 4.0 (Zenodo metadata) | the classic web corpus; all binary .xls, so our reader refuses every file until a .xls reader or an Excel conversion exists |
| Enron spreadsheets | https://doi.org/10.6084/m9.figshare.1221767 file `spreadsheets.7z` (https://ndownloader.figshare.com/files/3242531), md5 verified | 992.6 MB | 15,929 (15,871 .xlsx, 58 OLE2) | CC BY 4.0 (figshare metadata) | real business models; the archive holds .xlsx (converted from the originals) |
| Apache POI test-data | https://github.com/apache/poi, `trunk`, revision b66bb9a; paths in `sources/libs/20261111/poi.paths` | in `poi.tar.gz` | 819 (of 821 spreadsheet-typed paths) | Apache-2.0 (project licence) | edge cases for formula evaluation and file-format corners; many .xls |
| LibreOffice core test data | https://github.com/LibreOffice/core, revision 5f10c31, all spreadsheet-typed paths | in `libreoffice.tar.gz` | 935 .xls/.xlsx/.xlsm/.xlsb (of 1,732 paths, the rest .ods and templates) | mixed copyleft project; GitHub record GPL-3.0; no per-file licence on the test documents | per-function QA documents written by another implementation; licence is the weakest of the set, treat as research input only |
| ClosedXML tests | https://github.com/ClosedXML/ClosedXML | in `closedxml.tar.gz` | 384 | MIT | .NET writer round trips and formula cases |
| XlsxWriter comparison files | https://github.com/jmcnamara/XlsxWriter | in `xlsxwriter.tar.gz` | 1,005 | BSD-2-Clause | files saved by Excel 2007 to 2016 that the library must reproduce byte for byte; each feature in isolation, so ideal minimal cases |
| calamine tests | https://github.com/tafia/calamine | in `calamine.tar.gz` | 142 | MIT | .xls/.xlsb/.xlsx reader fixtures, including binary corner cases |
| ExcelJS test data | https://github.com/exceljs/exceljs | in `exceljs.tar.gz` | 37 | MIT | JavaScript reader fixtures (few, large) |
| Excelize test data | https://github.com/qax-os/excelize | in `excelize.tar.gz` | 9 | BSD-3-Clause | Go library fixtures |

R2 holds, below `sources/`: `euses/20261111/` (13 files, 194,138,085 bytes), `libs/20261111/` (15
files, 95,804,829 bytes: seven tarballs, seven path lists, `SOURCE.json`) and `enron/20261111/` (3
files, 992,636,485 bytes: the archive, its SHA-256 and `SOURCE.json`); each checked with
`rclone check --one-way` (0 differences). The unpacked copies on the workers are working copies and
can be deleted.

## Downloaded locally for Excel capture (owner direction, 2026-11-13)

These extracted files and per-file source lists stay on the kode-fi host because `rclone` is unavailable there; classification is complete. The coordinator copies them to the Windows capture machine. Counts and licence evidence are in [`corpus-sources.md`](corpus-sources.md), with per-file path, size and SHA-256 in the manifests.

| Corpus | Source | Downloaded files | Extracted bytes | Licence | What it adds |
|---|---|---:|---:|---|---|
| Sheetpedia XLSX | [Hugging Face dataset](https://huggingface.co/datasets/tianzl66/Sheetpedia_xlsx), pinned at `8d0e9c1d5f68e2389094bf739b1b7345a6822dee` | 211,323 | 5,501,687,718 | CC BY-SA 4.0 | Formula-rich Excel forum workbooks; streamed from one tar.gz archive |
| BigSpill onboarding workbooks | [GitHub repository](https://github.com/Patrick2788/BigSpill-Excel-Lambda-Library/tree/ad38a3b256219279371d2cdbb9ef39f0b585777e) | 11 | 2,214,673 | MIT | LET, LAMBDA and dynamic arrays |
| Replicator Dynamics workbook | [Zenodo record](https://zenodo.org/records/19467833) | 1 | 52,900 | CC BY 4.0 | Formula-heavy simulation sheet |

## Not downloaded

| Corpus | Source (exact) | Size | Files | Licence as stated | What it would add | Note |
|---|---|---:|---:|---|---|---|
| FUSE | https://zenodo.org/records/581678 `fuse.zip` | 9,392,830,118 bytes | 249,376 | CC BY 4.0 (Zenodo) | breadth of web spreadsheets, with JSON metadata that lets a formula-dense subset be chosen first | does not fit koderbot (about 21 GB free) next to the 15 GB reserve; needs the dc-kapl.6 archive decision or firehorse |
| VEnron2 | https://doi.org/10.6084/m9.figshare.4798042 `VEnron2.7z` | 315,143,533 bytes | not counted | CC0 (figshare metadata) | versions of Enron workbooks, for edit and recalculation scenarios | mostly overlaps Enron by content |
| Enron emails (not needed) | same figshare item, `eml.7z.001` to `.013` | about 9.4 GB | - | CC BY 4.0 | context only | skip |
| TmplEnron | https://figshare.com/articles/dataset/TmplEnron/5838600 | not read | - | not read | template-based Enron subset | check only if Enron proves useful |
| SheetJS test_files | https://github.com/SheetJS/test_files | - | - | - | the widest set of odd .xls/.xlsb/.xlsx files from many producers | GitHub answers "Repository access blocked" for the API; not fetched |
| openpyxl test data | foss.heptapod.net/openpyxl/openpyxl (Mercurial; the GitHub path is 404) | - | - | MIT (project) | writer and reader fixtures | not fetched: no git route |
| Ofgem, Ofwat, ONS models | see dc-ebb item 8 | few MB each | tens | OGL v3.0 where the site says so | large regulator models | unchanged from dc-ebb |

## How the sets would be used

* Captured and uncaptured populations stay separate. None of the downloaded files has an Excel
  capture; the "formula cells with a cached value" counts in the classification are saved-cache
  facts, never a pass.
* `.xls` (EUSES 4,608 files, about a quarter of the library sets) needs the Excel conversion step in
  dc-ztvv (open, SaveAs .xlsx, record `derived_from`) before our reader can open it.
* Selection for the first capture batch follows the profile: blockers out, then files that add
  function and feature combinations not yet captured.

## Harv5 source review and additions (2026-11-14)

These new research copies are under `~/corpora/`; each was SHA-256 deduplicated against `all-files.kode-fi.v5.jsonl`. Per-file inventories are in `corpus-manifests/*-20261114.tsv`; source URL, page, licence evidence, bytes and hashes are in each local `sources.tsv`. Counts below are static package/XML scans, not `cargo xtask classify`: this host has no Rust toolchain. They are inventory evidence, not Excel-calculated values.

### 1. Excel education creators and communities

| Set | Files / bytes | Licence class | Formula files / cells | VBA | Findings |
|---|---:|---|---:|---:|---|
| `contextures-20261114` | 2 / 139,052 | creator-download | 2 / 262 | 0 | One- and two-variable data tables; food-sales workbook has structured references, two PivotTables and two charts. |

Xelplus beginner practice material is email-delivered after a form, so it was skipped as gated; no personal information was entered. Contextures sample pages offer direct free downloads and state that files may not be resold; no open licence is claimed. A linked Contextures Web Radio `.xlsb` URL returned 404. Chandoo was unavailable to automated access under the fetcher's robots policy. The review also covered MrExcel, ExcelJet, Jon Peltier, Excelguru, Excel Campus, MyOnlineTrainingHub, Excel Off The Grid, Microsoft's template/sample pages, and VBA-oriented creator pages; no additional direct, ungated workbook with usable licence/source evidence was added. No mirrors of gated or paid material were used.

### 2. Code hosts

| Set | Files / bytes | Licence class | Formula files / cells | VBA | Findings |
|---|---:|---|---:|---:|---|
| `github-financial-dashboard-20261114` | 3 / 134,012 | open (MIT) | 1 / 8,772 | 2 `.bas` modules; no embedded project | Financial dashboard workbook with 3 tables and 5 charts; plain VBA source modules. |

The repository is pinned at `03bdcf0d2b7d48bb4b5cd643b8361dbe04d2c495`; the root MIT licence is recorded in `sources.tsv`. Both VBA modules returned `clean` from `~/vbawork/triage.py`'s code-analysis routine. It was run on plain `.bas` text with the parser dependency stubbed; this is not an embedded VBA-project scan. No other candidate with a clearly open workbook licence was added from the code-host search; this is not a claim that every host was exhaustively enumerated.

### 3. Other Excel-compatible engines and stated test corpora

| Set | Files / bytes | Licence class | Formula files / cells | Findings |
|---|---:|---|---:|---|
| `ironcalc-20261114` | 267 / 4,082,772 | open (MIT OR Apache-2.0) | 267 / 76,563 | `xlsx/tests` fixtures; dynamic arrays, LET/LAMBDA, structured refs, REGEX* and TRIMRANGE. |
| `xlcalc-20261114` | 7,171 / 108,942,424 | open (MIT) | 5,244 / 171,773 | README says tests compare 51,925 cases against stored Excel 16 results; fixtures include tables, pivots, charts, modern functions and file-structure edges. |
| `spreadsheetbench-v2-20261114` | 472 / 262,301,152 | open (MIT declared on dataset card) | 362 / 4,664,326 | Business-workbook benchmark; 803 charts, 108 data-table elements, 5 pivots. |

Already-held comparison sources: Apache POI, LibreOffice, ClosedXML, XlsxWriter, calamine, ExcelJS and Excelize test files are in the existing corpus/index. Apache POI's Excel-saved number-format cases are present there. HyperFormula documents ExcelJS for XLSX import and formula tests, not a separate compatibility workbook corpus. The `formulas` Python project documents `test/test_files/test.xlsx` as its function-coverage workbook and `test/test_files/excel.xlsx` as a model example; these repo fixtures were identified but not copied separately. Gnumeric, EPPlus, NPOI, pycel, xlcalculator, Koala and Spreadsheet::* were reviewed as candidate engine families; their tests were already represented by held sources, lacked clear separate file-level licensing, or did not state an external Excel conformance corpus. No new files were added from those projects.

### 4. Academic spreadsheet research

| Set | Files / bytes | Licence class | Formula profile | Findings |
|---|---:|---|---|---|
| `sestoft-finance2-20261114` | 1 / 54,846,990 | open (author states MIT-style) | XMLSS workbook; formula count not established by this inventory | Funcalc financial examples, custom functions and financial formulas. |
| `spreadsheetbench-v2-20261114` | 472 / 262,301,152 | open (MIT declared) | 362 formula files / 4,664,326 cells | New business task workbooks, also reported under source kind 3; not counted as a second set. |

Peter Sestoft's official Funcalc page offers `finance2.zip` and states an MIT-style open licence. EUSES, VEnron, Enron and Sheetpedia are already held and were not fetched again. SheetCopilot describes 221 evaluation tasks over 28 workbooks, but its README did not provide a data licence; pointer only. SpreadsheetBench v1 is recorded as a pointer, not duplicated. FUSE cut files remain out of scope and were not sought again. No TableBench corpus with separately downloadable Excel files and clear file-level terms was established in this pass.

R2 copies for the new sets are under `dnacalc-corpus/sources/<set>/20261114/`; copy and one-way check results are recorded in `~/codex-runs/harv5/report.md`.
