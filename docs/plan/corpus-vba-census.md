# VBA reference census (2026-11-14, v5)

Static census of SHA-256-unique local workbooks and the staged macro-capable R2 corpus objects. No Office program was started and no workbook was opened or executed. The v5 output and exact script used are in R2 at `dnacalc-corpus/meta/vba-census-20261114b/`.

### Reconciliation against R2

`rclone size` is recorded as object count / bytes. Status is relative to the v5 census; source bundles are mapped to their extracted or indexed census set.

| R2 folder | R2 size | Census result |
|---|---:|---|
| `sources/enron` | 3 / 992,636,485 | In `enron`; 15,927 unique workbooks, 0 VBA. Staged and extracted; SHA dedup added no workbooks. |
| `sources/euses` | 13 / 194,138,085 | In `euses`. |
| `sources/fuse` | 2 / 9,392,830,281 | In `fuse-vba`. |
| `sources/github-feature-examples-20261114` | 4 / 4,686,910 | In same named set; `.xlsx` only, cannot carry VBA. |
| `sources/github-vba` | 4 / 496,542,409 | In `new-github`. |
| `sources/github-vba-20261114` | 3 / 198,221 | In same named set. |
| `sources/github-vba-licensed-extra-20261114` | 2 / 3,546,739 | In same named set; 1 unique VBA workbook. |
| `sources/github-vba-tools-20261114` | 2 / 18,404 | In same named set. |
| `sources/libs` | 15 / 95,804,829 | In `closedxml`, `excelize`, `exceljs`, `libreoffice`, `poi`, and `xlsxwriter`; 142 calamine workbook files SHA-deduplicate under existing indexed labels. |
| `sources/mendeley-vba-20261114` | 5 / 96,829 | In same named set; 4 unique VBA workbooks. |
| `sources/sheetpedia-xlsx` | 3 / 3,808,105,032 | In `sheetpedia-xlsx`; all `.xlsx`, cannot carry VBA. |
| `sources/zenodo-vba` | 4 / 1,286,022,153 | In `new-zenodo`. |
| `windows/corpus` | 33,214 / 16,333,187,685 | Macro-capable workbook subset is in `r2-windows-corpus` (2,280); 7,218 listing-only `.xlsx` are non-macro-capable and remain undownloaded. Other objects are run/capture data. |

No other immediate folders were listed under `windows/`. Of the 7,218 listing-only `.xlsx`, the listing totals 437,109,051 bytes; they are counted separately and have no SHA identity in this census.

The v5 output copy to R2 passed `rclone check --one-way`: 5 files, 3,577,639 bytes, zero differences.

### Workbooks with VBA

v4 had 245,254 unique workbooks and 7,066 VBA workbooks. v5 adds 6 unique workbooks and 5 VBA workbooks (the four Mendeley workbooks and one licensed GitHub workbook from harv3; one non-VBA FUSE recovery file is also newly counted). The Enron source archive adds no unique hashes to the already extracted local set. XLM macro sheets remain separate: 28 workbooks.

| Set | Unique workbooks | VBA workbooks | VBA % |
|---|---:|---:|---:|
| `_downloads` | 2 | 2 | 100.00% |
| `closedxml` | 384 | 2 | 0.52% |
| `enron` | 15,927 | 0 | 0.00% |
| `euses` | 4,509 | 315 | 6.99% |
| `excelize` | 9 | 0 | 0.00% |
| `exceljs` | 37 | 0 | 0.00% |
| `fuse-vba` | 5,355 | 5,206 | 97.22% |
| `fuse-vba-full` | 1 | 0 | 0.00% |
| `github-feature-examples-20261114` | 3 | 0 | 0.00% |
| `github-vba-20261114` | 2 | 2 | 100.00% |
| `github-vba-licensed-extra-20261114` | 1 | 1 | 100.00% |
| `github-vba-tools-20261114` | 1 | 1 | 100.00% |
| `libreoffice` | 926 | 60 | 6.48% |
| `mendeley-vba-20261114` | 4 | 4 | 100.00% |
| `modern-lambda` | 11 | 0 | 0.00% |
| `new-github` | 2,648 | 729 | 27.53% |
| `new-zenodo` | 137 | 115 | 83.94% |
| `poi` | 811 | 56 | 6.91% |
| `r2-windows-corpus` | 2,280 | 580 | 25.44% |
| `sheetpedia-xlsx` | 211,323 | 0 | 0.00% |
| `xlsxwriter` | 984 | 0 | 0.00% |
| `zenodo-replicator-dynamics` | 1 | 0 | 0.00% |
| **Overall** | **245,260** | **7,071** | **2.88%** |

### Readable projects

Of the 7,071 VBA projects, 3,901 were read completely. Parse status: 3,107 FUSE files cut at 1 MiB, 52 with no PROJECTMODULES record, 8 truncated inside a record, 2 other OLE failures, and 1 bad ZIP. An unreadable project is never covered. The R2 output includes these parse counts and the full per-project rows.

### Top references

VBA and Excel are implicit (never stored in the dir stream); stdole and Office are stored. Control references are keyed by their original libid, so MSForms controls count as MSForms. Percentages use the 3,901 readable v5 projects.

| Library | GUID | Projects | % readable | Versions |
|---|---|---:|---:|---|
| stdole | `{00020430-0000-0000-C000-000000000046}` | 3,865 | 99.1% | 2.0 |
| Office | `{2DF8D04C-5BFA-101B-BDE5-00AA0044DE52}` | 3,839 | 98.4% | 2.0, 2.2 |
| MSForms | `{0D452EE1-E08F-101A-852E-02608C4D0BB4}` | 1,466 | 37.6% | 0.0, 2.0 |
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

### Which libraries cover the most workbooks

The v5 coverage pass was stopped after RSS reached 1.52 GB, above the 1.5 GB limit. No v5 coverage result is claimed. The table below is the last completed v4 baseline (7,066 VBA projects; 3,868 readable projects after leaving out our own 28 test workbooks). An unreadable project is never covered. The full v4 output is retained under `coverage.json` in the v5 R2 bundle, clearly nested as `v4_baseline`.

| k | Four always-present given (VBA, Excel, stdole, Office) plus | Covered | All references counted | Covered |
|---:|---|---:|---|---:|
| 1 | MSForms | 3,513 (90.8%) | stdole | 21 (0.5%) |
| 2 | MSForms, Scripting | 3,541 (91.5%) | stdole, Office | 2,214 (57.2%) |
| 3 | MSForms, Scripting, VBIDE | 3,559 (92.0%) | stdole, Office, MSForms | 3,513 (90.8%) |
| 4 | MSForms, Scripting, VBIDE, MSXML2 | 3,577 (92.5%) | stdole, Office, MSForms, Scripting | 3,541 (91.5%) |
| 5 | MSForms, Scripting, VBIDE, MSXML2, WinHttp | 3,594 (92.9%) | stdole, Office, MSForms, Scripting, VBIDE | 3,559 (92.0%) |

In v4, with the four given, 2,214 projects (57.2%) needed nothing more. MSForms alone brought this to 90.8%; the next libraries added under 1% each. The remaining 7% was a long tail of 200+ distinct reference sets.

## Method

The census hashes workbook files under `~/corpora/` (set names from the host index), adds the staged R2 `.xlsm`/`.xlsb`, and deduplicates by SHA-256. VBA is detected from `vbaProject.bin` in OOXML packages and from the VBA storage in OLE2 files; XLM from macro-sheet parts and BIFF records. Each project's `dir` stream is decompressed (oletools) and read record by record (MS-OVBA 2.3.4.2): project name, module count, REFERENCENAME, REFERENCEREGISTERED, REFERENCEPROJECT, REFERENCECONTROL and REFERENCEORIGINAL. No module source is read or emitted.

An earlier pass (harv2) read only 3,904 of 7,022 projects through a patched oletools class and searched coverage only over libraries that are some workbook's whole reference set; both are corrected here, as is a keying defect that split MSForms controls into one key per machine cache GUID.
