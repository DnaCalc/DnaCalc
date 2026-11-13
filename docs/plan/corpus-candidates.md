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
