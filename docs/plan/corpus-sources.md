# Public corpus sources and licences

These public files are held as local research copies under `~/corpora/`; workbook contents are not committed. Per-file path, byte count and SHA-256 are in [`corpus-manifests/`](corpus-manifests/). The source URLs and licence evidence are also recorded in each local set's `sources.tsv`.

| Set | Source | Licence | Licence evidence | Files | Extracted bytes | Coverage focus |
|---|---|---|---|---:|---:|---|
| `sheetpedia-xlsx` | [Sheetpedia XLSX dataset](https://huggingface.co/datasets/tianzl66/Sheetpedia_xlsx), pinned at `8d0e9c1d5f68e2389094bf739b1b7345a6822dee` | CC BY-SA 4.0 | [Dataset card](https://huggingface.co/datasets/tianzl66/Sheetpedia_xlsx/blob/8d0e9c1d5f68e2389094bf739b1b7345a6822dee/README.md) | 211,323 | 5,501,687,718 | Formula-rich forum workbooks; broad Excel functions and structures |
| `modern-lambda` | [BigSpill Excel Lambda Library](https://github.com/Patrick2788/BigSpill-Excel-Lambda-Library/tree/ad38a3b256219279371d2cdbb9ef39f0b585777e), pinned commit | MIT | [Root LICENSE](https://github.com/Patrick2788/BigSpill-Excel-Lambda-Library/blob/ad38a3b256219279371d2cdbb9ef39f0b585777e/LICENSE) | 11 | 2,214,673 | LET, LAMBDA, dynamic arrays and array functions |
| `zenodo-replicator-dynamics` | [Replicator Dynamics simulation workbook](https://zenodo.org/records/19467833) | CC BY 4.0 | [Zenodo record](https://zenodo.org/records/19467833) | 1 | 52,900 | Shared formulas and calculation chains |
| `github-vba-20261114` | [ExportVBACode](https://github.com/jjlong150/ExportVBACode/tree/b21c7ca1fcff2cbe61759f8b34135d3dc7984fc8); [data-collection-xlsx](https://github.com/dtkrizak/data-collection-xlsx/tree/67b0d1b906c97e38193594723315322d83b64cec) | MIT | [ExportVBACode LICENSE](https://github.com/jjlong150/ExportVBACode/blob/b21c7ca1fcff2cbe61759f8b34135d3dc7984fc8/LICENSE); [data-collection-xlsx LICENSE](https://github.com/dtkrizak/data-collection-xlsx/blob/67b0d1b906c97e38193594723315322d83b64cec/LICENSE) | 2 | 197,495 | VBA export utility and lab data collection workbook |
| `github-feature-examples-20261114` | [ExcelExamples](https://github.com/Excel-Bart/ExcelExamples/tree/55e902d6495664ac2041b53e4d62cbfe12b52955); [excel-financial-controlbook](https://github.com/mbimarban/excel-financial-controlbook/tree/593f301ef10185cd89a2b44364e51bd4766417a4) | MIT | [ExcelExamples LICENSE](https://github.com/Excel-Bart/ExcelExamples/blob/55e902d6495664ac2041b53e4d62cbfe12b52955/LICENSE); [controlbook LICENSE](https://github.com/mbimarban/excel-financial-controlbook/blob/593f301ef10185cd89a2b44364e51bd4766417a4/LICENSE) | 3 | 4,685,719 | Slicers, PivotTables, dynamic arrays and structured references |
| `github-vba-tools-20261114` | [ExcelMacros](https://github.com/ogadit/ExcelMacros/tree/6dd62342fd9711a1d83b865186ede2c6a58c1f91) | MIT | [Root LICENSE](https://github.com/ogadit/ExcelMacros/blob/6dd62342fd9711a1d83b865186ede2c6a58c1f91/LICENSE) | 1 | 18,045 | Macro shortcuts and workbook events |
| `mendeley-vba-20261114` | [VBA codes for data analysis dataset](https://data.mendeley.com/datasets/2rbp5x8hf7/2) | CC BY 4.0 | [Dataset licence and metadata](https://data.mendeley.com/datasets/2rbp5x8hf7/2) | 4 | 95,725 | Macro-enabled HTE/ABS and panel-data workbooks |
| `github-vba-licensed-extra-20261114` | [smart-student-inout-management-system](https://github.com/ankits-4/smart-student-inout-management-system/tree/a76fffff44f2300b448679d0eaaf8392e7d1c863) | MIT | [Pinned LICENSE](https://github.com/ankits-4/smart-student-inout-management-system/blob/a76fffff44f2300b448679d0eaaf8392e7d1c863/LICENSE) | 1 | 3,546,301 | VBA student log, tables, PivotTables and slicer; static triage suspect (stomping signal) |

The Sheetpedia archive was downloaded as one 3.69 GB tar.gz file and streamed to disk member by member; the archive itself is retained at `~/corpora/sheetpedia-xlsx-source.tar.gz`. The AutoXL demo workbook was hash-identical to an existing corpus file and was skipped.

The Mendeley set contains four unique `.xlsm` workbooks (95,725 bytes); all four have VBA projects and passed harvested-set static triage clean. Its files contain no formula cells. The source dataset describes the workbooks and reports CC BY 4.0. Their local source metadata is `~/corpora/mendeley-vba-20261114/sources.tsv`; all four project rows were added to the local VBA census. The set copy passed a one-way R2 check.

The additional GitHub MIT set has one unique `.xlsm` (3,546,301 bytes), 2,115 formula cells and 28 VBA modules. Static triage marked it suspect for a VBA-stomping signal, so it remains distinct from clean candidates. Its classifier found two tables, seven PivotTables and one slicer. The set adds one workbook to the project census; an exact coverage refresh was left at the earlier 7,022-project snapshot due memory use. A second workbook considered from `WilliamSmithEdward/ModernJsonInVBA` was SHA-identical to an existing `new-github` workbook and skipped. The MIT-licensed GitHub file copy and its `sources.tsv` passed R2 one-way checking.

A follow-up source search for the remaining feature gaps found one additional accepted-licence GitHub workbook with a slicer (recorded above), but no new data-table or REGEXTEST/REGEXREPLACE/GROUPBY/PIVOTBY workbook. The MIT-licensed ExcelExamples source already contributes slicer examples. The discovered `Kuba27x/Excel-19` data-table workbook has no repository licence, so it was not added. The indexed corpus now has 60 data-table files, 29 slicer files, 42 GROUPBY files and 19 PIVOTBY files; no REGEXTEST/REGEXREPLACE files were found.

The 2026-11-14 sets are local under `~/corpora/` and copied to R2 under `dnacalc-corpus/sources/<set>/`; each copy passed `rclone check --one-way` with zero differences. Source commits, workbook URLs, licences, evidence URLs, sizes and hashes are in the local `sources.tsv`; per-workbook path, bytes and SHA-256 are in the manifests above. Three GitHub workbook candidates were SHA-identical to existing corpus files and were skipped, including `ModernJsonInVBA.xlsm` (already in `new-github`).

Harv2 capture lists for the new formula-bearing workbooks are in `~/codex-runs/harv2/capture-batches/`; they are not committed because they contain public file paths.

The Sheetpedia manifest (path, bytes, sha256 for 211,323 files, 33.5 MB) is not committed because of its size. It is in R2 as `dnacalc-corpus/sources/sheetpedia-xlsx/20261113/manifest.tsv`, beside the source archive and `sources.tsv`; the other sets keep their manifests in `docs/plan/corpus-manifests/`.

### FUSE files cut at the Common Crawl size limit

The indexed FUSE-derived set on kode-fi is `fuse-vba` (5,355 files). `fuse-vba-named` is a hard-link mirror of those files, not another indexed set. Container checks used `olefile` for OLE and a complete `zipfile` read for ZIP files; this set contains OLE files only. The size-band results are:

| Size band (bytes) | Files | OLE opens | OLE invalid | ZIP files |
|---|---:|---:|---:|---:|
| `<1,040,000` | 2,135 | 2,133 | 2 | 0 |
| `1,040,000–1,048,575` | 3,186 | 65 | 3,121 | 0 |
| `1,048,576` | 34 | 1 | 33 | 0 |
| `>1,048,576` | 0 | 0 | 0 | 0 |

The cut class is the 3,154 OLE failures in the 1,040,000–1,048,576 byte band. Report it separately from valid-container and readable-workbook headline denominators. The two OLE failures below that band are not marked as cuts. Their hashes, set name and sizes are in `~/corpus-meta/fuse-truncated.tsv`; `~/corpus-meta/all-files.kode-fi.v2.jsonl` adds `"truncated": true` to those index rows and leaves the original index intact. This cap explains the 3,107 VBA project rows already reported as `file-cut-at-1MiB`; the broader file class also includes cut files without a readable VBA project.

The [FUSE project documentation](https://static.barik.net/fuse/index.html) describes 249,376 unique files from Common Crawl (Winter 2013–December 2014), supplies per-record URL/WARC metadata and the deduplicated binary archive, and points to its source code. The FUSE archive is distributed under CC BY 4.0 ([Zenodo record 581678](https://zenodo.org/records/581678)). The archive was downloaded from the existing R2 copy and passed `rclone check --one-way` (one file, zero differences). A streaming prefix-hash scan of its 249,376 tar members found 2,136 matches to cut-file prefixes: 13 complete OLE files open and were recovered to `~/corpora/fuse-vba-full/` with per-file `sources.tsv`; 2,123 prefix matches still fail OLE open, and 1,018 cut hashes have no matching prefix in the archive. The full archive is therefore a source for 13 files, but does not contain valid full copies for the other cut hashes. FUSE metadata can identify source URLs/WARC records; no separate file-level licence evidence was found for re-fetching the unmatched files from their original websites, so those were not downloaded. The recovery counts are also recorded in `~/corpus-meta/fuse-recovery.json`.

**Removed 2026-11-14 (owner decision).** The 3,154 cut files were deleted from `~/corpora/fuse-vba/` and its hard-link mirror `fuse-vba-named/` on kode-fi (6,308 paths; each file's size and SHA-256 checked against `fuse-truncated.tsv` first). Reason: the files were cut at about 1 MiB by the Common Crawl crawler, the FUSE archive holds the same cut bytes, and they will not be sought from other sources. The records stay: `~/corpus-meta/fuse-removed-20261114.tsv` (sha256, set, bytes, removed paths, reason, time) and index `all-files.kode-fi.v5.jsonl`, where those rows carry `"truncated": true` and a `removed` field with the reason. `fuse-vba` now holds 2,201 files; the 13 recovered full files in `fuse-vba-full/` stay. The original archive `r2:dnacalc-corpus/sources/fuse/20261111/fuse.zip` is unchanged. Census counts after this date exclude the removed files.

## Classification results

| Set | Formula files | Formula cells | VBA projects | XLM macro sheets | Added coverage |
|---|---:|---:|---:|---:|---|
| `sheetpedia-xlsx` | 73,260 | 65,107,798 | 0 | 0 | 4,482 LET/LAMBDA; 866 dynamic-array or spill; 57 data-table; 2,615 iterative; 6,558 structured-reference; 3,503 pivot; 10,226 table files |
| `modern-lambda` | 11 | 514 | 0 | 0 | 11 LET/LAMBDA and dynamic-array or spill files |
| `zenodo-replicator-dynamics` | 1 | 2,714 | 0 | 0 | Formula-bearing simulation workbook |
| `github-vba-20261114` | 1 | 5 | 2 | 0 | 1 clean and 1 suspect VBA workbook |
| `github-feature-examples-20261114` | 3 | 794 | 0 | 0 | 2 slicer files, 3 PivotTable files, 2 spill files, 1 structured-reference file |
| `github-vba-tools-20261114` | 0 | 0 | 1 | 0 | 1 clean VBA workbook; no formula-bearing workbooks |
| `github-vba-licensed-extra-20261114` | 1 | 2,115 | 1 | 0 | 1 suspect VBA workbook; 7 PivotTables and 1 slicer |
| `mendeley-vba-20261114` | 0 | 0 | 4 | 0 | 4 clean VBA workbooks; no formula-bearing workbooks |

Counts describe files in the new sets; overlap between feature counts is expected. Sheetpedia also contains XLOOKUP (1,587 files), FILTER (4,587), SORT (1,348), TEXTSPLIT (593), REGEXEXTRACT (2), GROUPBY (42), and PIVOTBY (19). No REGEXTEST or REGEXREPLACE formulas were found. The classifier opened 211,305 Sheetpedia packages; 18 did not open. All 211,323 files were XLSX, and the VBA-project and XLM scans found none.

Windows capture lists are in the lane output directory `~/codex-runs/harv1/capture-batches/`; they contain formula-bearing, macro-free files at or below 20 MiB, grouped by the thin features and modern functions. The lists are not committed because they contain public file paths; repository manifests retain only path, bytes and SHA-256.

## Harv5 additions (2026-11-14)

New local sets are SHA-256 deduplicated against `all-files.kode-fi.v5.jsonl`; manifests record path, bytes and SHA-256. `sources.tsv` in each set records original file URL, source page, licence evidence and file hash. Counts below are static OOXML/XML scans because Rust/cargo is not installed on this host; these are inventory counts, not the repository classifier or Excel-calculated results.

| Set | Source / licence | Files / extracted bytes | Formula files / cells | Other profile |
|---|---|---:|---:|---|
| `contextures-20261114` | Contextures samples; creator-download | 2 / 139,052 | 2 / 262 | 1 one-variable and 1 two-variable data table; 1 structured-ref workbook; 2 pivots, 2 charts |
| `github-financial-dashboard-20261114` | raniqn96/advanced-excel-financial-dashboard, pinned commit; MIT | 3 / 134,012 | 1 / 8,772 | 3 tables, 5 charts; two plain `.bas` modules, no embedded VBA project |
| `ironcalc-20261114` | IronCalc `xlsx/tests`, pinned commit; MIT OR Apache-2.0 | 267 / 4,082,772 | 267 / 76,563 | 10 tables, 1 pivot, 4 charts; dynamic arrays, LET/LAMBDA, structured refs, REGEX*, TRIMRANGE |
| `xlcalc-20261114` | drakosha/xlcalc, pinned commit; MIT | 7,171 / 108,942,424 | 5,244 / 171,773 | 619 tables, 3,616 pivot parts, 219 charts; includes 1,287 dynamic-array formula cells, 369 LET/LAMBDA, 117 GROUPBY/PIVOTBY, 26 TRIMRANGE and 1 IMAGE use |
| `spreadsheetbench-v2-20261114` | KAKA22/SpreadsheetBench-v2, pinned dataset revision; MIT declared | 472 / 262,301,152 | 362 / 4,664,326 | 3 tables, 5 pivots, 803 charts, 108 data-table XML elements |
| `sestoft-finance2-20261114` | Funcalc / Peter Sestoft official page; author-stated MIT-style | 1 / 54,846,990 | XMLSS; not counted by OOXML scan | Financial spreadsheet examples; formula count not established |

The spreadsheet files above total 7,916 files and 430,446,402 bytes; `.bas` source modules are included in the GitHub set's file count and bytes. The count for academic source kind 4 includes SpreadsheetBench v2 again as a cross-listed source, not as additional files. The new workbook sets contain no embedded macro-enabled projects; two standalone VBA source modules were triaged `clean` by `~/vbawork/triage.py`'s code analysis. The `oletools` parser dependency was unavailable, so no embedded project triage was run on this batch.

The engine/research review also found that `formulas` documents `test/test_files/test.xlsx` (COVERAGE sheet) as its coverage workbook and `test/test_files/excel.xlsx` as a model fixture. Those fixtures were not copied separately. HyperFormula documents XLSX import through ExcelJS, not a separate Excel conformance workbook corpus. Apache POI, LibreOffice, ClosedXML, XlsxWriter, calamine, ExcelJS and Excelize fixture sets were already indexed. SheetCopilot's README cites 221 tasks over 28 workbooks without a data licence; pointer only. EUSES, VEnron, Enron and Sheetpedia are held already. FUSE cut files were not searched for or re-fetched.

### Interesting-example index

The local `~/corpus-meta/interesting-examples.tsv` currently has 11 entries: 1 data-table case, 1 structured-reference/PivotTable/chart combination, 1 tables/charts example, 2 VBA modules, 1 dynamic-array spill example, 2 recursive LAMBDA examples, 1 TRIMRANGE case, 1 REGEX* case and 1 GROUPBY case. It records hashes and workbook/module locations, without copying workbook contents into the repository.

## Archive pass 2026-11-16 (dc-w3zu0)

Owner direction 2026-11-16: store each corpus in R2 as its source distributes it, with its metadata, without unpacking or inspecting any file, and with no Excel conversion for now. Every file was hashed whole (SHA-256, and compared with the source's own checksum where one is published), copied with `rclone copy --checksum` and checked with `rclone check --one-way` (0 differences for every prefix). Each prefix holds a `SOURCE.json` (download URLs, pinned revision, licence with its wording and where it is stated, other terms, the source's own claims about its contents, citation, overlap with held sets) and a `meta/` folder with the landing pages and API records as fetched. The run folder is `~/codex-runs/corpus-archive-20261116/` on kode-fi (report, logs, scripts); no local corpus copies are kept. `sources/` in `dnacalc-corpus` now holds 12,319 objects, 41,815,287,128 bytes.

### Stored (prefix `sources/<set>/20261116/`)

| Set | Files | Bytes | Licence as stated | Source checksums | What the source says it holds |
|---|---:|---:|---|---|---|
| `fuse` (project-page files) | 7 | 9,740,628,208 | CC BY 4.0 on the Zenodo record; the project page states none for these files | 7 of 7 SHA-1 matched | 4 analysis JSON.gz files, SHA-1 list, dedup map, 6.9 GB binaries tarball; 2,127,284 URLs, 249,376 unique files (Common Crawl, 2013 to 2014). Whether the tarball repeats `fuse.zip` is not known without opening either |
| `enron` (email volumes) | 13 | 8,859,674,182 | CC BY 4.0 (figshare) | 13 of 13 MD5 matched | `eml.7z.001` to `.013`, one multi-volume 7z: the emails with the original attachments |
| `venron1-0` | 1 | 64,878,068 | CC0 (figshare 4797943) | MD5 matched | 360 evolution groups, 7,294 spreadsheets |
| `venron1-1` | 1 | 156,037,432 | CC0 (figshare 4797961) | MD5 matched | 322 groups, 7,171 spreadsheets (fixes to 1.0) |
| `venron2` | 1 | 315,143,533 | CC0 (figshare 4798042) | MD5 matched | 1,609 groups, 12,254 spreadsheets (SpreadCluster) |
| `tmplenron` | 1 | 16,516,099 | CC BY 4.0 (figshare 5838600) | MD5 matched | 47 templates with 490 instances; 21 instance-template groups |
| `sheetjs-enron-xls` | 1 | 1,396,470,057 | CC0 1.0 (LICENSE file) | none published | GitHub archive of commit 5b73fc39: Enron spreadsheets in original formats (BIFF2 to BIFF8, TSV, SYLK, HTML saved as .xls), de-duplicated by MD5 |
| `spreadsheetbench-v1` | 4 | 110,716,218 | CC BY-SA 4.0 (card) | 4 of 4 matched | 912 tasks and 2,729 test cases; Verified, 400 tasks |
| `spreadsheet-rl` | 16 | 1,240,089,950 | CC BY-SA 4.0 (card) | 16 of 16 matched | 5,925 ExcelForum training tasks plus test parts that reuse SpreadsheetBench and others |
| `gdp-xlsx` | 34 | 740,708 | Apache-2.0 (card; asks to be kept out of training) | 34 of 34 matched | 3 of the 70 tasks |
| `finch-finworkbench` | 537 | 175,072,804 | CC BY 3.0 (card) | 537 of 537 matched | 172 workflows, 1,710 spreadsheets, 27 million cells, built on Enron, EUSES and public-sector files |
| `npoi-20261116` | 1 | 89,282,263 | Apache-2.0 | none published | pinned GitHub archive |
| `phpspreadsheet-20261116` | 1 | 1,067,186 | MIT | none published | pinned GitHub archive; holds no spreadsheets (`.gitattributes` export-ignores `/tests` and `/samples`) |
| `pandas-20261116` | 1 | 5,702,023 | BSD-3-Clause | none published | pinned GitHub archive; holds no spreadsheets (`.gitattributes` export-ignores the spreadsheet extensions) |
| `open-xml-sdk-20261116` | 1 | 46,118,826 | MIT | none published | pinned GitHub archive |
| `umya-spreadsheet-20261116` | 1 | 8,515,805 | MIT | none published | pinned GitHub archive |
| `aspose-cells-for-net-20261116` | 1 | 53,876,464 | MIT (the examples repository, not the product) | none published | pinned GitHub archive |
| `formulas-20261116` | 1 | 633,863 | EUPL-1.1 | none published | pinned GitHub archive |
| `format-corpus-20261116` | 1 | 560,845,440 | CC0 per the README, "unless otherwise stated" | none published | Open Preservation Foundation format-corpus, pinned GitHub archive |
| `ofgem-price-cap-20261116` | 199 | 1,537,419,746 | Crown copyright under the Open Government Licence (Ofgem copyright page; the version is not written there, the link goes to OGL v3.0) | none published; sizes equal HTTP Content-Length | per quarter: cap levels, Annexes 2, 3, 4, 5 and 9, demand and losses workbook, monthly rates; the 3 workbooks held since 2026-10 were not fetched again |
| `desnz-appraisal-20261116` | 3 | 2,211,654 | OGL v3.0 (gov.uk) | none published | appraisal toolkit (.xlsm), data tables, emissions-savings template |
| `hmt-green-book-20261116` | 3 | 123,162 | OGL v3.0 (gov.uk) | none published | appraisal summary tables, discount factors, financial annex template |
| `govdocs1` (metadata only) | 10 metadata files | 14,200,000 (about) | site metadata CC0; corpus "may be (to the best of our knowledge) freely redistributed" | SHA-1 and MD5 of the 1,000 zips saved | about 1 million files in 1,000 zips (333,494,682,740 bytes in all, from the site index); the Forensic Innovations report counts 62,876 OLE Excel files; by-type archives `xls.zip` (8,050,349,823 bytes) and `xlsx.zip` are offered |

### Pointers only (prefix `sources/_pointers/<set>/20261116/`: landing page, record and SOURCE.json, no corpus files)

| Set | Why no files |
|---|---|
| `modified-euses-tugraz`, `info1-tugraz`, `integer-corpus-tugraz`, `payroll-gradebook-tugraz`, `enron-errors` | no licence on the source pages |
| `custodes`, `deco-tu-dresden` | host does not answer, or the dataset pages are gone |
| `panko-student-spreadsheets` | a paper, no dataset release |
| `sheetcopilot`, `bluefin`, `realhitbench` | non-commercial terms |
| `fxbench` | card says "other", its Croissant file says CC BY-SA 4.0 |
| `auto-formula-microsoft` | data under the Microsoft Research License; owner 2026-11-16: we do not use it or ask for permission. Its Cisco, PGE and TI sets were crawled from the public web, so look for those origins instead. Later we may suggest it to an independent researcher as a way to check DNA Calc against Excel |
| `sheetjs-test-files` | no licence for the zip as a whole; GitHub blocks the repository for "private_information"; the zip host (341,452,051 bytes) sets robots content signal `ai-train=no` |
| `ofwat-pr24` | HTTP 403 browser check |
| `gnumeric-20261116`, `openpyxl-20261116` | the hosts' robots.txt disallow archive downloads |

### Notes from the pass

- kode-fi reaches figshare only over IPv4 (IPv6 gets 403); use `curl -4`. figshare web pages return a bot-challenge page, so only API records are saved for figshare items.
- The VEnron project host does not resolve; VEnron 1.0 and 1.1 are CC0 figshare deposits, stored above.
- PhpSpreadsheet's and pandas' test spreadsheets are not in their GitHub archives; storing them needs a pinned git clone of the test folders, packed as the 2026-11-11 library sets were.
- The Touchstone catalogue's note that TmplEnron has no public release is wrong: figshare 5838600 exists.

## GovDocs1 Excel files (2026-11-16, dc-ztvv.16)

Owner direction 2026-11-16: get GovDocs1, keep its Excel files, discard the rest. GovDocs1 ([Digital Corpora](https://digitalcorpora.org/corpora/file-corpora/files/); Garfinkel et al., DFRWS 2009) is about a million US government files. Its terms put the site content and metadata under CC0 and say the corpus files "may be (to the best of our knowledge) freely redistributed"; that is the curators' statement, not a per-file licence.

| R2 prefix (`dnacalc-corpus/`) | Objects | Bytes | What it holds |
|---|---:|---:|---|
| `sources/govdocs1/20261116-by-type/` | 3 | 8,056,055,359 | `xls.zip` (8,050,349,823 bytes) and `xlsx.zip` (5,702,506 bytes) as distributed, SHA-256 equal to the site index; SOURCE.json |
| `sources/govdocs1/20261116-meta2/` | 8 | 368,521,802 | `dump.sql` (per-file metadata database, 298,928,989 bytes), `govdocs1-first512-first4096-docid.txt`, the site's `bin/` scripts, HEADER and Makefile, all SHA-256 equal to the index; SOURCE.json |
| `sources/govdocs1-excel/20261116/` | 62,676 | 30,104,365,665 | the 62,672 extracted files (`xls/NNN/NNNNNN.xls`, `xlsx/NNN/NNNNNN.xlsx`), `manifest.tsv` (path, bytes, SHA-256, source archive, member, container class), `sources.tsv`, `SOURCE.json`, `extract_failures.tsv` (empty) |

The extracted set is 62,672 files, 30,071,088,567 bytes: 62,635 from `xls.zip` and 37 from `xlsx.zip`. By first bytes only (no workbook opened): 60,058 OLE2, 2,579 raw BIFF (pre-OLE BIFF2 to BIFF5), 35 ZIP; two of the OLE2 files are named `.xlsx`. 1,562 SHA-256 values occur more than once: GovDocs1 is not de-duplicated. Every upload batch passed `rclone check --one-way`, and the R2 counts equal the manifest. The files are not kept on kode-fi (they would take free disk below 20 GB); `~/corpora/govdocs1-excel/` holds the manifest, `sources.tsv`, `SOURCE.json` and `extract_failures.tsv`, and the files come back with `rclone copy`.

The by-type archives select by file name. A sample of 10 of the 1,000 corpus zips (`000.zip`, `100.zip`, ... `900.zip`; 9,845 members) found every `.xls` and `.xlsx` member in the by-type set by SHA-256, and 7 spreadsheet files under other names: 4 SpreadsheetML 2003 XML files (three `.xml`, one `.html`) and 3 raw BIFF files named `.unk`. Times 100, that is roughly 700 more files in the whole corpus, about 1.1 percent of the by-type set (the Forensic Innovations report counts 507 SpreadsheetML files). The full scan would download all 333 GB of corpus zips for those files, so it was not run. Run log, scripts and per-zip results: `~/codex-runs/govdocs1-excel-20261116/` on kode-fi.

## Small sets copied to R2 (2026-11-16, dc-ztvv.19)

The kode-fi housekeeping check found two open-licensed sets from 2026-11-14 with no copy in R2. Both are now stored, and both passed `rclone check --one-way`:

| Prefix | Files | Bytes | Licence |
|---|---:|---:|---|
| `sources/ironcalc-20261114/20261116/` | 268 | 4,205,981 | MIT OR Apache-2.0 (IronCalc test workbooks, commit 53c00566) |
| `sources/modern-lambda/20261116/` | 12 | 2,219,551 | MIT (BigSpill LAMBDA library, commit ad38a3b2) |

Every other `~/corpora` set on kode-fi either matches its R2 prefix file for file or has its source archive in R2. The exceptions are `exceloffthegrid-local-20261115` (personal-use EULA, so it stays local only) and `zenodo-replicator-dynamics` (53 KB).
## Cross-corpus deduplication (2026-11-17, dc-ztvv.17)

Every Excel-family file in `sources/` (46 sets, all but `_pointers/`) is in one index, with one copy of each distinct file. Run on dna-firehorse. Only containers and first bytes were read: no workbook was parsed, opened or converted. A file counts as Excel-family if its content shows it (an OLE2 Workbook or Book stream, a zip with `xl/workbook.xml` or `.bin`, SpreadsheetML 2003, raw BIFF2-5) or if it has an Excel-family extension. CSV, ODS and Lotus files are counted per set, not indexed.

| Prefix | Objects | Bytes | Contents |
|---|---:|---:|---|
| `sources/xl-unique/20261116/<xx>/<sha256>.<ext>` | 607,119 | 63,399,170,875 | one copy per distinct file; the extension follows the content class |
| `runs/dna-firehorse/20261116-xl-dedup/` | 73 | about 371 MB | `files.tsv` (one row per distinct file), `origins.tsv` (996,929 occurrences, with archive or e-mail member chains), `sets.tsv`, `overlap.tsv`, `errors.tsv` (54 damaged members), `scope.tsv`, `SOURCE.json`, scripts, check logs, report |

**Totals.** 996,929 occurrences of 607,119 distinct files (63.4 GB):
- 288,166 zip workbooks;
- 256,887 OLE2 workbooks;
- 58,420 OLE2 files from GovDocs1 whose Workbook stream was not checked, because their class comes from the GovDocs1 manifest;
- 2,650 raw BIFF files;
- a few hundred text, HTML or other files with Excel extensions.

**Checks.**
- Each upload batch passed `rclone check --one-way`.
- GovDocs1's files were copied within R2 and matched on size and MD5.
- The store's listing equals `files.tsv` key for key, and every size matches.
- A random sample of 400 files re-hashed to their names.

**Largest contributors of distinct files.**
- FUSE: 239,788 (its two binary tarballs are the same files).
- Sheetpedia: 211,323, all unique to it.
- GovDocs1: 61,110.
- The Enron e-mail attachments: 18,362.
- Spreadsheet-RL: 32,635.

**Overlap.**
- The `.xls` attachments read from the Enron e-mails contain SheetJS enron_xls (17,062 shared), VEnron2 (12,206), VEnron 1.0 (7,228) and VEnron 1.1 (7,140), and add 1,300 files none of those has.
- The older `enron/20261111/spreadsheets.7z` is a converted `.xlsx` copy (15,927 files). It shares no bytes with the originals.
- Spreadsheet-RL contains 2,745 SpreadsheetBench v1 files and 290 from v2.
- TmplEnron shares fewer than 100 files with any set.

**Extensions.** 479,580 occurrences have no extension, nearly all FUSE binaries. 1,327 `.xls` files are really text.

The internals catalogue (dc-ztvv.18) reads from the unique store. Run folder on dna-firehorse: `~/codex-runs/xl-dedup-20261116/`.

## Harv7 tips and tricks (2026-11-21, dc-ebb.3)


The figures are for payload files. R2 object counts include source manifests, occurrence lists
and licence notices. The private prefixes are `sources/<set>/20261121/`. For each copied
set, `rclone copy --checksum` and `rclone check --one-way` were used. Creator downloads
remain test material for local use. Their verified R2 copies are private, with no
redistribution licence.

| Set | Licence class | Licences and files | Payload files | Payload bytes | R2 check, objects and bytes |
|---|---|---|---:|---:|---|
| [creator-tips-local-20261121](corpus-manifests/creator-tips-local-20261121.tsv) | creator-download | creator-download (675) | 675 | 19,882,153 | 676 / 20,170,499 |
| [github-gist-tips-20261121](corpus-manifests/github-gist-tips-20261121.tsv) | open | MIT (2) | 2 | 28,088 | 3 / 29,517 |
| [github-licensed-tips-20261121](corpus-manifests/github-licensed-tips-20261121.tsv) | open | MIT (317), Apache-2.0 (119), GPL-3.0 (9), Unlicense (1) | 446 | 65,825,087 | 465 / 66,304,777 |
| [microsoft-office-client-docs-20261121](corpus-manifests/microsoft-office-client-docs-20261121.tsv) | open | MIT (9) | 9 | 2,415 | 13 / 33,777 |
| [microsoft-vba-docs-20261121](corpus-manifests/microsoft-vba-docs-20261121.tsv) | open | MIT (3,199) | 3,199 | 656,924 | 3,203 / 4,174,639 |
| [stackexchange-tips-20261121](corpus-manifests/stackexchange-tips-20261121.tsv) | open | CC BY-SA 4.0 (1,350), CC BY-SA 3.0 (2,558), CC BY-SA 2.5 (216) | 4,124 | 1,409,139 | 4,125 / 2,912,495 |

Per-file provenance and licence evidence are in `~/corpora/<set>/sources.tsv` on kode-fi
and with the matching R2 set. The Microsoft occurrence lists also preserve source locations
for byte-identical examples and excerpts of results stated by their pages. The Microsoft
code licence is MIT. The documentation text and extracted result excerpts carry CC BY 4.0.
The new files were hashed against `all-files.kode-fi.v5.jsonl` and one another. The v6 index
preserves the v5 records and appends the new files. Two matches appeared in older standalone
manifests outside v5, one Stack Exchange snippet and one Excel Easy workbook.

## Foundation external corpora index (2026-03)

Migrated from [Foundation `EXTERNAL_CORPORA_INDEX.csv`](https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/test-corpus/EXTERNAL_CORPORA_INDEX.csv).
HyperFormula (`EXT-TC-001`), Apache POI (`EXT-TC-002`), LibreOffice (`EXT-TC-003`), Gnumeric
(`EXT-TC-005`), and Calamine (`EXT-TC-006`) are already indexed in the main sets above; the
remaining pointers from that index are:

| ID | Name / author | URL | Licence status | Focus |
| --- | --- | --- | --- | --- |
| `EXT-TC-004` | `formulas.info` examples | https://www.formulas.info/ | `reference_only` (not for verbatim extraction) | Educational Excel formula shapes |
| `EXT-TC-007` | `pbartxl` gists (Peter Bartholomew) | https://gist.github.com/pbartxl | `public_gist` (check per-gist terms before copying) | 6 gists, ~30+ named LAMBDA definitions (thunks, data dictionaries, combinatorics, FIFO, array-of-arrays) |
| `EXT-TC-008` | `pbartxl` Lambda functions repo | https://github.com/pbartxl/PBart-s-Lambda-functions | `public` (PR #1; no repo licence verified here) | Cooley-Tukey `FFTλ`/`IFFTλ`/`Convolveλ` in pure Excel LAMBDA; Foundation adapted self-contained `LET` variants in `tools/gen_corpus_fft.py` |
