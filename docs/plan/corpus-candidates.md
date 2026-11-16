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

Update 2026-11-16 (dc-w3zu0): the archive pass stored FUSE's project-page files, the Enron email
volumes, VEnron 1.0, 1.1 and 2, TmplEnron and the Ofgem models in R2 as compressed downloads, and
recorded SheetJS test_files and openpyxl as pointers only. The table below is the 2026-11-11 view;
the current state of each source is in [`corpus-sources.md`](corpus-sources.md), section "Archive
pass 2026-11-16".

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

## Harv6 second corpus pass (2026-11-14–15)

The deeper review followed creator sample indexes, all nine requested GitHub search terms, official Microsoft sample links, and academic citation trails. New data stays under `~/corpora/`; manifests list only path, bytes and SHA-256. `sources.tsv` carries per-file provenance and licence classification. Counts are static inventories, never Excel calculation results. The existing `cargo xtask classify` and VBA triage could not run on this host (`cargo` and `oletools` are absent); workbooks received a streamed ZIP/XML feature scan, while VBA triage is marked unavailable where projects are present.

### Creator samples and companion files

The direct public downloads found in this pass are inventoried in `~/corpora/creators-20261114/sources.tsv` and `~/corpora/exceloffthegrid-local-20261115/sources.tsv`; the corresponding repository manifests contain only path, bytes and SHA-256. The creator-download class means freely downloadable for local test use without an open licence. Excel Off The Grid is a local-only exception: its [EULA](https://exceloffthegrid.com/eula/) limits use to individual/personal use and bars hosting or transfer, so this set was not copied to R2.

| Site | Files / bytes in this pass | Licence class | Formula files / cells | Embedded VBA projects | Static findings |
|---|---:|---|---:|---:|---|
| Contextures | 0 new (2 from Harv5) | creator-download | — | — | Root and `www` returned 502 after normal browser-UA retry. |
| Peltier | 0 | — | — | — | Accessible homepage/sitemap had no downloadable files; site search is disallowed by robots. |
| Excelguru | 0 retained; 2 moved outside the corpus after terms review | — | — | — | Its [terms](https://www.excelguru.ca/about/terms-of-use/) prohibit reproducing/copying without written consent. |
| MrExcel | 0 | — | — | — | Accessible homepage has no companion-file index or sitemap; robots disallows search and attachment paths. |
| ExcelJet | 188 / 22,246,743 | creator-download | 187 / 18,237 | 0 | Dynamic arrays 726 formula hits; LAMBDA 146; LET 171; GROUPBY 34; PIVOTBY 15; REGEX 73; TRIMRANGE 9; IMAGE 14; structured references 357. |
| Excel Off The Grid | 438 / 33,146,453 | creator-download, local-only | 200 / 10,635 | 52 | Dynamic arrays 375; LAMBDA 53; LET 57; GROUPBY 23; PIVOTBY 2; REGEX 2; TRIMRANGE 11; IMAGE 11; structured references 7,797; 483 tables, 20 pivots, 74 charts and 51 slicer parts. |
| MyOnlineTrainingHub | 0 | — | — | — | Robots returned 403 twice, including browser-UA retry; one resource also required an email form. |
| Excel Campus | 0 | — | — | — | Its [legal terms](https://www.excelcampus.com/legal/) prohibit copying or duplication without written consent. |
| Ron de Bruin | 0 | — | — | — | Robots URL redirected to `/history/` after browser-UA retry; no downloads collected. |
| Chip Pearson | 0 | — | — | — | Site TLS hostname mismatch on both attempts; Wayback archive request returned 429 twice. |
| wellsr | 3 / 49,736 | creator-download | not scanned | unknown | Three directly linked XLS workbooks; static OOXML formula/VBA profiling does not parse legacy XLS. |
| Excel Macro Mastery | 0 | — | — | — | Robots request disconnected twice, including browser-UA retry. |

Excel Off The Grid's allowed `/insiders-downloads/` index exposed 236 file URLs. All returned successfully; ZIP members were inspected statically and only workbook/VBA source files were retained. The 438-file set includes 382 XLSX and 56 XLSM files. The classifier does not yet inspect legacy XLS internals; no office application was used. The external VBA triage script could not run because `oletools` is absent, so the 52 embedded projects are inventoried but not triaged. Excelguru's two pre-review files remain outside the corpus and R2; they are excluded from counts and manifests.

### Microsoft templates, support/learn samples and code repositories

| Set | Files / bytes | Licence | Findings |
|---|---:|---|---|
| `microsoft-20261114` | 1 / 10,233 | creator-download | Official PivotTables/Office Scripts article's fruit-sales source workbook. Static package contains only workbook data sheets; it does not include saved PivotTable or slicer objects. |
| `microsoft-officedev-20261114` | 3 / 181,398 | open (MIT; pinned `OfficeDev/Office-Add-in-samples` commit `b6c210d649509dff3dfe6b7d6c2f2ceb81540a32`) | Two real XLSX files contain four structured tables; a third tracked as `.xlsx` has a BIFF `.xls` signature and was preserved without conversion. |
| `microsoft-afm-20261114` | 5 / 13,958 | open (MIT; pinned `microsoft/advanced-formula-environment` commit `0b35bab12c08251b7825cc98ef341a822de00215`) | Five Markdown formula-library code examples, all with LAMBDA; two also use LET and four use dynamic-array helpers. No `.lambda` or `.txt` module files exist at this revision. |

Reviewed Microsoft Support/Learn pages for dynamic/legacy arrays, LAMBDA, LET, XLOOKUP, PivotTables, Power Query and data types. The array-formula example workbook link uses `aka.ms`, disallowed by its robots policy. Three Import Data/Data Model workbook links, two Student Data Model workbook links and one Power BI Financial Sample workbook link target `download.microsoft.com`; its robots endpoint redirected to a Microsoft download landing page, so none was fetched. The `create.microsoft.com` route led to an M365 portal and yielded no direct workbook download. The reviewed function pages did not offer additional standalone workbook downloads. Static profiling found no formula cells or embedded VBA in these four Microsoft workbooks; AFM's five Markdown files contain five LAMBDA, two LET and four dynamic-array helper code examples. The fruit-sales workbook above is linked from the [Microsoft PivotTables article](https://learn.microsoft.com/en-us/office/dev/scripts/develop/pivottables); the [array formula article](https://support.microsoft.com/en-gb/excel/guidelines-and-examples-of-array-formulas) remains a pointer for its blocked download.

### Academic citation trails and spreadsheet benchmarks

| Source trail | Files added | Licence / outcome |
|---|---:|---|
| Held Hermans, Sestoft/Funcalc, EUSES, VEnron, Sheetpedia, SpreadsheetBench v2 | 0 new | Existing sets were cross-checked; Sestoft's `finance2` workbook is already in the Harv5 set, and the others were not downloaded again. |
| SpreadsheetBench v1 | 0 | Official Hugging Face card lists CC BY-SA 4.0 and a 111 MB archive. Its download redirected to `us.aws.cdn.hf.co`; the CDN robots policy disallows the target path, so the archive was not fetched. The project's [GitHub README](https://github.com/Reality2byte/spreadsheetbench#license-and-citation) states CC BY-SA 4.0; citation only because the permitted download route blocked. |
| TACO formula-graph corpus | 0 | The paper describes Enron and GitHub-scraped spreadsheets. Enron files are already held; no file-level licence was found for the scraped files. |
| TUTA / ForTaP | 0 | The paper reports that the underlying source spreadsheets are not public. |
| SpreadsheetCoder | 0 | The paper describes 46,000 internal Google Sheets; no public workbook set or data licence is offered. |
| SEMS spreadsheet benchmarks | 0 | Info1, Integer, modified EUSES and Payroll/Gradebook download pages expose files but do not state a file licence; links retained as pointers only. |
| TAT-QA and other table-QA trails | 0 | TAT-QA states CC BY 4.0, but its cited source material is financial-report PDF tables rather than workbooks. HiTab's MIT repo license covers code; its Excel originals are under CUD v1.0, not an open licence, so no workbook copy was made. |

Additional pointers: [TUTA/ForTaP paper](https://pmc.ncbi.nlm.nih.gov/articles/PMC11322654/), [SpreadsheetCoder paper](https://storage.googleapis.com/gweb-research2023-media/pubtools/6222.pdf), [SEMS corpus index](https://spreadsheets.sai.tugraz.at/index.php/corpora-for-benchmarking/), [TACO paper](https://arxiv.org/abs/2302.05482), [TAT-QA repository](https://github.com/NExTplusplus/tat-qa), and [HiTab](https://github.com/microsoft/HiTab). The existing SpreadsheetBench v2 files were SHA-256 duplicates and were skipped.

### Code-host searches

GitHub REST repository search covered `topic:vba`, `topic:excel-vba`, `topic:excel-macros`, `topic:vba-excel`, `topic:excel-template`, `topic:excel-dashboard`, `topic:excel-lambda`, `topic:dynamic-arrays`, and `language:VBA`, paced across the unauthenticated search quota. Of 239 repositories with an allowed licence response, 162 repositories contained target files and were collected at pinned commit SHAs. `sources.tsv` records repository page, raw file URL, commit, licence and licence-file URL per file.

| Set | Files / bytes | Licence-class detail | File extensions |
|---|---:|---|---|
| `github-licensed-20261114` | 3,426 / 686,911,716 | open: MIT 2,757; GPL-3.0 298; Apache-2.0 272; CC0-1.0 66; AGPL-3.0 17; BSD-3-Clause 10; CC BY 4.0 6 | `.bas` 1,743; `.xlsx` 803; `.cls` 563; `.xlsm` 164; `.frm` 131; `.xlsb` 10; `.xls` 9; `.xltm` 2; `.xltx` 1 |

The clone was shallow and blob-filtered; only matching workbook/VBA files were checked out, then each temporary clone was removed. GitLab REST search was disallowed by its robots policy and its project-search UI returned no server-rendered matches. Codeberg's search route was disallowed by robots. SourceForge directory search returned 403 on the initial request and browser-UA retry. No alternate host or mirror was used.

Static ZIP/XML profiling of the 989 workbook files found 296 formula-bearing workbooks / 753,952 formula cells, including 36,358 dynamic-array formula hits, 7 LAMBDA hits and 348,590 structured-reference hits. Packages included 136 VBA projects, 385 tables, 127 PivotTables, 226 charts, 46 slicer parts and one data-table XML node. These are package/formula counts, not calculated outcomes; the 2,437 standalone VBA modules were not triaged because `oletools` is unavailable.

### R2 and interesting examples

Copy plus `rclone check --one-way` found 0 differences for the Microsoft sets (2, 4 and 6 matching files), GitHub (3,427 matching files) and the eligible creator set (192 matching files, including `sources.tsv`). Excel Off The Grid's 438-file set stays local-only under its EULA; it was not copied. The 38 rows in `~/corpus-meta/interesting-examples.tsv` cover the requested formula families, plus structured references/tables, charts, PivotTables, slicers, What-If data tables, embedded VBA and standalone modules. Category totals overlap when a single example shows multiple features: dynamic arrays/spills 6; LAMBDA/LET 9; GROUPBY 3; PIVOTBY 2; IMAGE 2; REGEX 3; TRIMRANGE 3; structured references/tables 5; PivotTables/charts/slicers 3; What-If data tables 2; VBA modules/projects 3.
