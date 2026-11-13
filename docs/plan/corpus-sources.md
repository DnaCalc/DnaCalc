# Public corpus sources and licences

These public files are held as local research copies under `~/corpora/`; workbook contents are not committed. Per-file path, byte count and SHA-256 are in [`corpus-manifests/`](corpus-manifests/). The source URLs and licence evidence are also recorded in each local set's `sources.tsv`.

| Set | Source | Licence | Licence evidence | Files | Extracted bytes | Coverage focus |
|---|---|---|---|---:|---:|---|
| `sheetpedia-xlsx` | [Sheetpedia XLSX dataset](https://huggingface.co/datasets/tianzl66/Sheetpedia_xlsx), pinned at `8d0e9c1d5f68e2389094bf739b1b7345a6822dee` | CC BY-SA 4.0 | [Dataset card](https://huggingface.co/datasets/tianzl66/Sheetpedia_xlsx/blob/8d0e9c1d5f68e2389094bf739b1b7345a6822dee/README.md) | 211,323 | 5,501,687,718 | Formula-rich forum workbooks; broad Excel functions and structures |
| `modern-lambda` | [BigSpill Excel Lambda Library](https://github.com/Patrick2788/BigSpill-Excel-Lambda-Library/tree/ad38a3b256219279371d2cdbb9ef39f0b585777e), pinned commit | MIT | [Root LICENSE](https://github.com/Patrick2788/BigSpill-Excel-Lambda-Library/blob/ad38a3b256219279371d2cdbb9ef39f0b585777e/LICENSE) | 11 | 2,214,673 | LET, LAMBDA, dynamic arrays and array functions |
| `zenodo-replicator-dynamics` | [Replicator Dynamics simulation workbook](https://zenodo.org/records/19467833) | CC BY 4.0 | [Zenodo record](https://zenodo.org/records/19467833) | 1 | 52,900 | Shared formulas and calculation chains |

The Sheetpedia archive was downloaded as one 3.69 GB tar.gz file and streamed to disk member by member; the archive itself is retained at `~/corpora/sheetpedia-xlsx-source.tar.gz`. The AutoXL demo workbook was hash-identical to an existing corpus file and was skipped.

The Sheetpedia manifest (path, bytes, sha256 for 211,323 files, 33.5 MB) is not committed because of its size. It is in R2 as `dnacalc-corpus/sources/sheetpedia-xlsx/20261113/manifest.tsv`, beside the source archive and `sources.tsv`; the other sets keep their manifests in `docs/plan/corpus-manifests/`.

## Classification results

| Set | Formula files | Formula cells | VBA projects | XLM macro sheets | Added coverage |
|---|---:|---:|---:|---:|---|
| `sheetpedia-xlsx` | 73,260 | 65,107,798 | 0 | 0 | 4,482 LET/LAMBDA; 866 dynamic-array or spill; 57 data-table; 2,615 iterative; 6,558 structured-reference; 3,503 pivot; 10,226 table files |
| `modern-lambda` | 11 | 514 | 0 | 0 | 11 LET/LAMBDA and dynamic-array or spill files |
| `zenodo-replicator-dynamics` | 1 | 2,714 | 0 | 0 | Formula-bearing simulation workbook |

Counts describe files in the new sets; overlap between feature counts is expected. Sheetpedia also contains XLOOKUP (1,587 files), FILTER (4,587), SORT (1,348), TEXTSPLIT (593), REGEXEXTRACT (2), GROUPBY (42), and PIVOTBY (19). No REGEXTEST or REGEXREPLACE formulas were found. The classifier opened 211,305 Sheetpedia packages; 18 did not open. All 211,323 files were XLSX, and the VBA-project and XLM scans found none.

Windows capture lists are in the lane output directory `~/codex-runs/harv1/capture-batches/`; they contain formula-bearing, macro-free files at or below 20 MiB, grouped by the thin features and modern functions. The lists are not committed because they contain public file paths; repository manifests retain only path, bytes and SHA-256.
