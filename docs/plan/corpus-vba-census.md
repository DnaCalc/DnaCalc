# VBA reference census (2026-11-14)

Static census of every SHA-256-unique workbook in the corpus trees on kode-fi plus the downloaded R2 `windows/corpus/` macro-capable files (dc-boou). No Office program was started and no workbook was executed. Script: `tools/corpus-vba-census/census.py`. Outputs (`projects.jsonl` one row per workbook-project with its full reference list, `references.tsv`, `coverage.json`, `summary.json`, and the script) are in R2 at `dnacalc-corpus/meta/vba-census-20261114/`.

## Workbooks with VBA

| Set | Unique workbooks | VBA workbooks | VBA % |
|---|---:|---:|---:|
| `_downloads` | 2 | 2 | 100.00% |
| `closedxml` | 384 | 2 | 0.52% |
| `enron` | 15,927 | 0 | 0.00% |
| `euses` | 4,509 | 315 | 6.99% |
| `excelize` | 9 | 0 | 0.00% |
| `exceljs` | 37 | 0 | 0.00% |
| `fuse-vba` | 5,355 | 5,206 | 97.22% |
| `github-feature-examples-20261114` | 3 | 0 | 0.00% |
| `github-vba-20261114` | 2 | 2 | 100.00% |
| `github-vba-tools-20261114` | 1 | 1 | 100.00% |
| `libreoffice` | 926 | 60 | 6.48% |
| `modern-lambda` | 11 | 0 | 0.00% |
| `new-github` | 2,648 | 729 | 27.53% |
| `new-zenodo` | 137 | 115 | 83.94% |
| `poi` | 811 | 56 | 6.91% |
| `r2-windows-corpus` | 2,280 | 580 | 25.44% |
| `sheetpedia-xlsx` | 211,323 | 0 | 0.00% |
| `xlsxwriter` | 984 | 0 | 0.00% |
| `zenodo-replicator-dynamics` | 1 | 0 | 0.00% |
| **Overall** | **245,254** | **7,066** | **2.88%** |

XLM macro sheets: 28 workbooks (counted apart from VBA). `fuse-vba` was selected for VBA, so its 97% says nothing about workbooks at large; `sheetpedia-xlsx` (211,323 `.xlsx`) cannot hold VBA. `r2-windows-corpus` includes 28 of our own test workbooks (OxVba_TestEventServer). The R2 listing also has 7,218 `.xlsx` (437 MB) counted from listing metadata only (not downloaded, not deduplicated, no VBA possible).

## Readable projects

Of the 7,066 VBA projects, 3,896 were read completely. 3,107 are FUSE files cut at 1 MiB by the FUSE crawl (dc-afua: 3,107 of the 3,109 OLE failures are 1,040,000-1,048,576 bytes), 52 have no PROJECTMODULES record, 8 end inside a record, and 3 fail otherwise (2 OLE, 1 bad zip). Percentages below use the 3,896 readable projects. An unreadable project is never counted as covered.

## Top references

VBA and Excel are implicit (never stored in the dir stream); stdole and Office are stored. Control references are keyed by their original libid, so MSForms controls count as MSForms.

| Library | GUID | Projects | % of readable | Versions |
|---|---|---:|---:|---|
| stdole | `{00020430-0000-0000-C000-000000000046}` | 3,860 | 99.1% | 2.0 |
| Office | `{2DF8D04C-5BFA-101B-BDE5-00AA0044DE52}` | 3,834 | 98.4% | 2.0, 2.2 |
| MSForms | `{0D452EE1-E08F-101A-852E-02608C4D0BB4}` | 1,465 | 37.6% | 0.0, 2.0 |
| Scripting | `{420B2830-E718-11CF-893D-00A0C9054228}` | 86 | 2.2% | 1.0 |
| VBIDE | `{0002E157-0000-0000-C000-000000000046}` | 36 | 0.9% | 5.0, 5.3 |
| MSXML2 | `{F5078F18-C551-11D3-89B9-0000F81FE221}` | 31 | 0.8% | 3.0, 6.0 |
| OxVba_TestEventServer | `{E2A30001-0001-0001-0001-000000000001}` | 28 | 0.7% | 1.0 |
| RefEdit | `{00024517-0000-0000-C000-000000000046}` | 24 | 0.6% | 1.0, 1.2 |
| ADODB | `{B691E011-1797-432E-907A-4D8C69339129}` | 20 | 0.5% | 6.1 |
| VBScript_RegExp_55 | `{3F4DACA7-160D-11D2-A8E9-00104B365C9F}` | 18 | 0.5% | 5.5 |
| WinHttp | `{662901FC-6951-4854-9EB2-D9A2570F2B2E}` | 18 | 0.5% | 5.1 |
| CICLib | `{3D5905E0-523C-11D1-9FEA-00600832DB4A}` | 13 | 0.3% | 1.0 |
| MSForms | `{92C78480-A79F-11D0-8ADF-00A0C91110EB}` | 9 | 0.2% | 2.0 |
| MSHTML | `{3050F1C5-98B5-11CF-BB82-00AA00BDCE0B}` | 8 | 0.2% | 4.0 |
| IWshRuntimeLibrary | `{F935DC20-1CF0-11D0-ADB9-00C04FD58A0B}` | 8 | 0.2% | 1.0 |
| Word | `{00020905-0000-0000-C000-000000000046}` | 7 | 0.2% | 8.3, 8.6, 8.7 |
| Outlook | `{00062FFF-0000-0000-C000-000000000046}` | 7 | 0.2% | 9.3, 9.5, 9.6 |
| MSComctlLib | `{831FDD16-0C5C-11D2-A9FC-0000F8754DA1}` | 7 | 0.2% | 2.0, 2.1, 2.2 |
| SHDocVw | `{EAB22AC0-30C1-11CF-A7EB-0000C05BAE0B}` | 7 | 0.2% | 1.1 |
| mscorlib | `{BED7F4EA-1A96-11D2-8F08-00A0C9A6186D}` | 6 | 0.2% | 2.4 |

## Which libraries cover the most workbooks

A project is covered by a library set when every one of its references is in the set. Exact search for each k (depth-first branch and bound over the distinct reference sets); greedy agreed with exact at every k. Our own 28 test workbooks are left out here (3,868 readable projects); with them in, OxVba_TestEventServer takes a slot at k >= 2.

| k | Four always-present given (VBA, Excel, stdole, Office) plus | Covered | All references counted | Covered |
|---:|---|---:|---|---:|
| 1 | MSForms | 3,513 (90.8%) | stdole | 21 (0.5%) |
| 2 | MSForms, Scripting | 3,541 (91.5%) | stdole, Office | 2,214 (57.2%) |
| 3 | MSForms, Scripting, VBIDE | 3,559 (92.0%) | stdole, Office, MSForms | 3,513 (90.8%) |
| 4 | MSForms, Scripting, VBIDE, MSXML2 | 3,577 (92.5%) | stdole, Office, MSForms, Scripting | 3,541 (91.5%) |
| 5 | MSForms, Scripting, VBIDE, MSXML2, WinHttp | 3,594 (92.9%) | stdole, Office, MSForms, Scripting, VBIDE | 3,559 (92.0%) |

With the four given, 2,214 projects (57.2%) need nothing more. MSForms alone brings this to 90.8%; the next libraries add under 1% each. The remaining 7% is a long tail of 200+ distinct reference sets (ADODB, VBScript RegExp, RefEdit, Word, Outlook, common controls, Solver, scripting runtimes).

## Method

The census hashes workbook files under `~/corpora/` (set names from the host index), adds the staged R2 `.xlsm`/`.xlsb`, and deduplicates by SHA-256. VBA is detected from `vbaProject.bin` in OOXML packages and from the VBA storage in OLE2 files; XLM from macro-sheet parts and BIFF records. Each project's `dir` stream is decompressed (oletools) and read record by record (MS-OVBA 2.3.4.2): project name, module count, REFERENCENAME, REFERENCEREGISTERED, REFERENCEPROJECT, REFERENCECONTROL and REFERENCEORIGINAL. No module source is read or emitted.

An earlier pass (harv2) read only 3,904 of 7,022 projects through a patched oletools class and searched coverage only over libraries that are some workbook's whole reference set; both are corrected here, as is a keying defect that split MSForms controls into one key per machine cache GUID.
