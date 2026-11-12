# Capture batch 1 (bead dc-hc85, epic dc-ztvv)

The first ~500 corpus files for an Excel capture, chosen so that one Excel minute buys the most new
(function, argument shape) pairs and formula features. This is a selection, not a capture: no Excel
ran, no corpus file was opened in Excel, and the batch waits for the owner's capture machine.

Selection matters only while Excel time is short. The plan is an ongoing Excel evaluation environment
that captures whole corpora (owner, 2026-11-11). Batch 1 is the first, fast-feedback slice, not the
target. The same selection later gives a small representative regression set that runs on every
change, while full corpora run on a schedule.

* **Inputs.** The classifier's `files.jsonl` (`cargo xtask classify`, build of this commit's parent
  plus the shape keys) for Enron (15,929 files; R2 `dnacalc-corpus/sources/enron/20261111/`) and the
  library test sets (3,331 files; `sources/libs/20261111/`, one archive per library). A hash gives its
  path in `docs/plan/corpus-manifests/<set>.tsv`. Covered baseline: the repository's own fixtures
  (`crates`, `platform`, `apps`, `tools`), whose 3,112 shape keys are not counted as new. EUSES is not in
  the selection: it is `.xls` only, which our reader does not open.
* **Reproduce.** `cargo xtask classify <dir> --recursive --out <o>` for each set and for the repository
  folders, then
  `cargo xtask capture-select --set enron=<enron>/files.jsonl --set libs=<libs>/files.jsonl
  --covered <repo>/files.jsonl --count 500 --out docs/plan/capture-batch-1.md`. The rules (what is
  excluded, the gain per file, the greedy choice, the time model) are the header of
  `tools/xtask-harness/src/capture_select.rs`.
* **Shape keys** are `NAME(tokens)` in Touchstone's vocabulary plus reference tokens
  (`docs/plan/corpus-classification.md`, "Argument shapes"); a trailing `!` marks position- or
  reference-sensitive functions, which must run on the file route. Pairs of `RAND`, `RANDBETWEEN` and
  `RANDARRAY` carry no gain. The shape of a cell reference does not say what type the cell holds, so a
  pair such as `IF(x,r,r)` can still cover several Touchstone questions once the capture reads the cells.
* **The time is a model** (3 s per file, 1.5 s per MB, 0.0002 s per formula cell), not a measurement.
  The first measured batch replaces it. Files that were chosen for diversity after every new pair was
  taken (a pair held by fewer than five chosen files still gains a quarter) fill the list to about 500.
* **Hashes only.** No file name, sheet name or content is listed.

Written by `cargo xtask capture-select` from the classifier's `files.jsonl`. Hashes only: no file names, sheet names or contents. Nothing here ran Excel or opened a corpus file; the batch waits for the capture machine.

## Result

* 486 files chosen of 6628 candidates (19260 files read).
* 629 new (function, argument shape) pairs, of 629 pairs the candidates hold that no covered file has (832 pairs in all among candidates); 12 new formula features or kinds.
* 1907594 formula cells to capture.
* Estimated Excel time: 33 minutes (0.5 hours), by the model 3 s per file + 1.5 s per MB + 0.0002 s per formula cell. It is a model; the first measured batch replaces it.
* Covered baseline: 3112 shape keys from 1 file list(s).

## Settings

Count 500, repeat 5, size limits 8388608 bytes and 300000 formula cells, sets: enron, libs.

## Excluded

| Reason | Files |
|---|---:|
| ActiveX controls, data connections or query tables | 195 |
| add-in or user-defined function calls | 37 |
| external links | 2867 |
| special handling in this batch (RTD, CUBE*, plus deferred external-service functions such as WEBSERVICE and STOCKHISTORY) | 2 |
| macros (VBA project, macro sheet, .xlsm or .xlsb) | 65 |
| no formula cells | 7943 |
| over the size limits | 1 |
| reader refuses the file (blocker) | 1499 |
| same SHA-256 as an earlier candidate | 23 |

## By source

| Source | Files chosen | New pairs | Est. minutes |
|---|---:|---:|---:|
| enron | 409 | 441 | 29 |
| libs | 77 | 188 | 4 |

## Files, in the order chosen

| # | Source | SHA-256 | Bytes | Formula cells | New pairs | New features | Est. s | Cum. min |
|---:|---|---|---:|---:|---:|---:|---:|---:|
| 1 | libs | 16a2f714110e538e505d566ff4fdf8267c49ac80b11f87b2b671bac894537b25 | 21127 | 517 | 60 | 1 | 3.1 | 0.1 |
| 2 | libs | b10c917495578a5f0238ec4f4c0d8cd457d8afda602cd23edc3642be6b0ee577 | 27708 | 639 | 26 | 5 | 3.2 | 0.1 |
| 3 | libs | e54cd64b614c9b4edddf09eaaadebb02b85c323afaad02097ef98ee64d841b2e | 108049 | 516 | 15 | 1 | 3.3 | 0.2 |
| 4 | enron | 64284ccc3db0b4a85eed1ab6716f56bd0f65bd586671bb6d339010967f5d1c35 | 763355 | 19002 | 30 | 1 | 7.9 | 0.3 |
| 5 | enron | 1c6892e6f4791a65f971447e95b66e833232337ab6dfdb40c3567973ce078ecc | 92456 | 2017 | 13 | 0 | 3.5 | 0.3 |
| 6 | enron | 084e46e73038c0b0a00a8c0a82444cdaf04ef2533bfb74b5595502e7e87cc92c | 97312 | 2702 | 13 | 0 | 3.7 | 0.4 |
| 7 | libs | 1f330777fce8b9c5144e014f2d84b3886e2d67fd0e61cc26ba9d0ddfd7167eba | 14300 | 276 | 10 | 0 | 3.1 | 0.5 |
| 8 | enron | dc5fe26308df4a91a5308d5c58428231323eced8be22222a682dc713243e1e7d | 254335 | 6867 | 9 | 2 | 4.7 | 0.5 |
| 9 | enron | 48ddb698b0d33e5b40f2ce2e4dec49f50dd22f79c3f25f4a2aa7003bf54fc8fa | 217914 | 6245 | 11 | 1 | 4.6 | 0.6 |
| 10 | enron | 728421500a3b79d21689edfb8e4926183fa0727e51999dd5c7c259d119016bb9 | 178042 | 3145 | 11 | 0 | 3.9 | 0.7 |
| 11 | enron | 27c04ee84b55c81e0ce518d7102555140503d4a21e57be04cbb9bba8352f7d4e | 279822 | 7102 | 13 | 0 | 4.8 | 0.8 |
| 12 | enron | 570be387273923cc9355b4ad3ac724e5120acac236e66c16f2a7094a9e5b83be | 163941 | 1292 | 8 | 0 | 3.5 | 0.8 |
| 13 | enron | 280f18c95ce7a2a0adf3b5ebb04f2d9128aea190f4fc6f58543e70d959321640 | 131830 | 550 | 8 | 0 | 3.3 | 0.9 |
| 14 | enron | 87787cc5cd02f4f3cc981793799405657a42c8bbbee18a329f2cc02bcd5bc87d | 78969 | 2236 | 8 | 0 | 3.6 | 0.9 |
| 15 | enron | 22b50d467361c936ece4fa4583f52da3a6b8795334297446de3d7d88aa8b2f29 | 65396 | 867 | 8 | 0 | 3.3 | 1.0 |
| 16 | libs | 12f1dcef889c20fe54a09857790ba9803a7b061cc9fc5b85d0038cd6c86faa79 | 14875 | 36 | 7 | 0 | 3.0 | 1.0 |
| 17 | enron | 9245003279fd1e13ad161c4f46ba982300e324036c1ea8a4dd6011038397ce90 | 121916 | 5941 | 9 | 0 | 4.4 | 1.1 |
| 18 | enron | 2a17cbc3c25201fb9bfc6689fac2cc4c374321b3710bb647caf2dcde366a2996 | 44489 | 1946 | 7 | 0 | 3.5 | 1.2 |
| 19 | enron | 593ab2a13f63be2b22c111a407f7a5f117fab812f4748dcac404ea25e2dc0b95 | 333399 | 12676 | 11 | 0 | 6.0 | 1.3 |
| 20 | enron | b99ac69108efa3a08567a40e99600c6c240c97d87c41be7113d5908d165547dd | 512875 | 19872 | 13 | 0 | 7.7 | 1.4 |
| 21 | libs | d1fd0530d65dc44849ee036425b481da39215c1bc6e562ae1b104c8b2998d195 | 18712 | 458 | 5 | 0 | 3.1 | 1.5 |
| 22 | enron | 93e09d1eea925acc87ebcd7aa21a76eec8e0ad13569aa8dce07a810992493757 | 216697 | 5091 | 7 | 0 | 4.3 | 1.5 |
| 23 | enron | 908841299b01efa809b34d84549f23d27839dea6277fb403b3855ec015ebb7a3 | 40831 | 176 | 6 | 0 | 3.1 | 1.6 |
| 24 | enron | daf4da5cfa7fa8b98ff73cb56cd796caf75cdc1984dac65c5ffb4a69f3064071 | 21305 | 21 | 5 | 0 | 3.0 | 1.6 |
| 25 | enron | 3998ed7f2ae8278bcbc2757cb1724e53182ab73eddf143a7da8aaa19dfa0f6ad | 57366 | 582 | 5 | 0 | 3.2 | 1.7 |
| 26 | enron | a8a9de7d56e4b2984f505cda14c143d959bd06aff084ced5e4a8050fce5132e7 | 483158 | 5518 | 7 | 0 | 4.8 | 1.8 |
| 27 | enron | 817d9073c78732cc45ed40adb534dce8597411cb88ae105f7cbffacc91e87f00 | 33269 | 529 | 5 | 0 | 3.2 | 1.8 |
| 28 | enron | 7bc3baf8b9303717f2881c9d55f22f82b5dc5ff9dbd0754551acff298871754f | 63086 | 407 | 4 | 0 | 3.2 | 1.9 |
| 29 | libs | 9995989ed73695c7b65c97e91bad3c673bc668c1f5a80b694f74a624fc50d746 | 8001 | 16 | 4 | 0 | 3.0 | 1.9 |
| 30 | enron | 3a668d6cc62fc80cfceb36bb65632e81168b7e74b623253393fd2aab166a674b | 899384 | 17084 | 7 | 0 | 7.7 | 2.0 |
| 31 | enron | ab9b0698e4e1d688127223cfe71d20b49bcf0768770d2a64e8630ad7a7ea71b9 | 24814 | 43 | 4 | 0 | 3.0 | 2.1 |
| 32 | libs | 32f3e668515150721147a1ee3a41c95b70beee7cc67dcbb005c8fca08f20d58f | 38593 | 7 | 4 | 0 | 3.1 | 2.1 |
| 33 | enron | ea0c83c4e2349de440f08cdd145f950fa4a3b8b781726acad81f2d49e68f1690 | 43632 | 128 | 4 | 0 | 3.1 | 2.2 |
| 34 | enron | c25000d49c8cf1890a303f15cb26af974e687913b93a5190a0bfbde9c79109c9 | 67789 | 504 | 4 | 0 | 3.2 | 2.2 |
| 35 | enron | e2bc050101562fcfa189554adda0442e567a8a750d9e6dcb250f90d70d46b358 | 238987 | 13379 | 6 | 0 | 6.0 | 2.3 |
| 36 | enron | b07f3a819550a5f11c1e4b36b8f87970ff278371675ce982de8c334985d0bb9d | 12385 | 128 | 3 | 0 | 3.0 | 2.4 |
| 37 | enron | bf415045f1d6e3faf7d5ac315ffb598b5049776f3dbfebd5e990863b3c529a41 | 17267 | 129 | 3 | 0 | 3.1 | 2.5 |
| 38 | enron | b6030e99875e9e905e6010fa8415910d590ddc8aade983e27876385b92dce492 | 99071 | 4919 | 4 | 0 | 4.1 | 2.5 |
| 39 | enron | 61d7989053ec4c6a908416aa1ad6640763abbfd479caa6ff5132819b08d54868 | 213495 | 1026 | 4 | 0 | 3.5 | 2.6 |
| 40 | enron | b366e3b90fabdf0dcb6cd32633a717b61ddf0c2e8a09e8e3ea727380206361f4 | 14652 | 14 | 3 | 0 | 3.0 | 2.6 |
| 41 | enron | 8fcc4c56511242d9309e1d47d1f47049027a2b3fdeb4d104b9cc8e70d5942116 | 153038 | 3314 | 1 | 1 | 3.9 | 2.7 |
| 42 | enron | 71e3241b37f55fa1c3a4398c2ac036844e3d52fa7893e00ae44be4ffc1145adf | 191652 | 9011 | 4 | 0 | 5.1 | 2.8 |
| 43 | enron | c45089777c61a8afae68937bb927207c99997910ac729d6b18bd16724506375c | 195646 | 6415 | 4 | 0 | 4.6 | 2.9 |
| 44 | libs | c48fc0c688dd276bdcccbe756a221821556653910bcd7b6c886a398c5a007ee2 | 9458 | 2 | 3 | 0 | 3.0 | 2.9 |
| 45 | enron | 6a01c333ff554e3940e5fea8c3bca8387481c89657c22918a3b5c6cf30172729 | 14697 | 24 | 3 | 0 | 3.0 | 3.0 |
| 46 | enron | a62efeff98ea9b44b3a03a92fef00a10e242e584b315160982a315311fbe4792 | 312852 | 6908 | 3 | 0 | 4.8 | 3.0 |
| 47 | enron | 3db8a669673f78384e7b00f19f530ea0c6db3d0022f9f63b43513fcae185a5d4 | 129622 | 3166 | 3 | 0 | 3.8 | 3.1 |
| 48 | enron | 8e4b5c1ee34d3ce3e660b1ea834b709495c235d50f6402284d7120c866cf2e28 | 111950 | 5004 | 4 | 0 | 4.2 | 3.2 |
| 49 | enron | a4c7765bccc753fb41f51c15c02993c67709fbf4a6b500899f6dfbac7508490e | 32267 | 458 | 2 | 0 | 3.1 | 3.2 |
| 50 | enron | aba371925c14b348873d6299fadc091057667e511262b5e7247662fefc9a6325 | 363930 | 3931 | 3 | 0 | 4.3 | 3.3 |
| 51 | enron | bae3b7619bfed54a56effa0ff5cd1450fa00438dbc0b91473c8574693cdfbe7d | 77699 | 829 | 2 | 0 | 3.3 | 3.3 |
| 52 | enron | 26e96443322bd8717d8c4d536805a608a4216002f6e51ee8c4347ac1c73b3cde | 99902 | 2214 | 3 | 0 | 3.6 | 3.4 |
| 53 | enron | b64b5fbee688f0c2b9ab6875ea9913c9e0c523b3122664f32d5207b1f702e202 | 61254 | 1850 | 2 | 0 | 3.5 | 3.5 |
| 54 | enron | 300ff4c83f782c82f9cc8ef55213d211901bd06062cc269df903b69d8d28c847 | 189306 | 12788 | 4 | 0 | 5.8 | 3.6 |
| 55 | enron | 3d3f486f4cc3018a8ff56a158434d84db032af6574a18c0fd8ca930b4203667e | 108852 | 3293 | 3 | 0 | 3.8 | 3.6 |
| 56 | enron | 1d2ebf6d1326c402a7431d40e6107b5a6da7268a34a3415ba84466e4524a9f30 | 90233 | 3800 | 3 | 0 | 3.9 | 3.7 |
| 57 | enron | afe752e9f2152066e3305cba5afbcda340d8abdcf0b96ba5e34b11ab9f843154 | 75063 | 4567 | 3 | 0 | 4.0 | 3.8 |
| 58 | enron | 4ed169aa7fd33e6e2d676d2401d6dff987f73b55cd0d58be9655f1f449ad610f | 63997 | 111 | 2 | 0 | 3.1 | 3.8 |
| 59 | enron | da5d4d036a2b60cf2b01efd7907c6c1310f2127cde6e8ab7005efd58f9f6979d | 48023 | 403 | 2 | 0 | 3.1 | 3.9 |
| 60 | enron | 40cfa0c5cebf6a8f72992455c8347b8c0c8264f274bf8c6877c4b109c50304a4 | 73038 | 609 | 2 | 0 | 3.2 | 3.9 |
| 61 | enron | e7276f900566162478521254e11d3bc92c53f60cc29cb571382ada7f5bee45c3 | 207148 | 13416 | 3 | 0 | 6.0 | 4.0 |
| 62 | enron | f68a46a380989467521f2a3e423dad58526d995f665e0a27dc0015c3e1c8c0ac | 208974 | 1793 | 2 | 0 | 3.7 | 4.1 |
| 63 | libs | 334a6ebff35265c31d7ada2729012da11169748416fab74e5804ef20906b231f | 13122 | 27 | 2 | 0 | 3.0 | 4.1 |
| 64 | enron | bdc6b2ffa5a728fb871bd4835d9c1544adfc70ff04626f89c1d944fcdc158c49 | 17984 | 4 | 2 | 0 | 3.0 | 4.2 |
| 65 | enron | e77ea4a7fdba09575625d711ff833a2ce321ff53d8ee2aa4691b30639b799402 | 15687 | 148 | 2 | 0 | 3.1 | 4.2 |
| 66 | enron | 893621daf4933f28515e7cf8363361674a1b9d30cc2dce787f4ed10d35a0bad5 | 25134 | 202 | 2 | 0 | 3.1 | 4.3 |
| 67 | enron | 817d80aafc9e17bf547f4041ef1b6bdcb2f135dec1df322eba8f5d0bb211c6f4 | 336730 | 3637 | 2 | 0 | 4.2 | 4.3 |
| 68 | enron | 7b8e4060fb1fefd0ecc1755a5ec4690d4fcc0821d3f6f9a0f3e2325ec3554871 | 156201 | 3186 | 2 | 0 | 3.9 | 4.4 |
| 69 | enron | 389ddb093dbcf43a04f1a8d632ec91f8f61bc972618ecdd2404b6b2e9e81f636 | 267536 | 9670 | 3 | 0 | 5.3 | 4.5 |
| 70 | libs | b21d7656fd79a2f129d496495743b5aa2a21641deb9fa86379b3d6316b39f97d | 6361 | 1 | 2 | 0 | 3.0 | 4.6 |
| 71 | libs | 2bc49ba351d246b2d9f91a57c02d3f72fc762ffd629046de6a7d40cb0aa4d92b | 7251 | 3 | 2 | 0 | 3.0 | 4.6 |
| 72 | libs | 8d0785e7cffa8c97b1860fa9028e0cadd0a4e1e798b649eb00a7b900bed30f2c | 7823 | 5 | 2 | 0 | 3.0 | 4.7 |
| 73 | libs | 5c7f47a903bfb7acdac72db42a9b2359ae4c48e77fa10a46c4233891458002ca | 8752 | 2 | 2 | 0 | 3.0 | 4.7 |
| 74 | libs | 3c82c9077998257be2ef1bdc56227b5825786cd75c48ea6af40c867bee49010c | 10247 | 2 | 2 | 0 | 3.0 | 4.8 |
| 75 | libs | 30f79c7c368fa4508ab3ce38002ae3c8499c68c94a2104012aefed37e84efddb | 11961 | 3 | 2 | 0 | 3.0 | 4.8 |
| 76 | libs | 4d8fd3db6469df14f280964b3a447e0be027eda0e2195ca62693e92d8d353910 | 12031 | 4 | 2 | 0 | 3.0 | 4.9 |
| 77 | enron | a38b9472802ec1d5695649469522b47fb08df5221b15eb110df6cae2d168787b | 14919 | 2 | 2 | 0 | 3.0 | 4.9 |
| 78 | libs | 8c881dcc60ee13d287292d23fd878b82e84517d72bb46443e5755c59278f9503 | 17584 | 18 | 2 | 0 | 3.0 | 5.0 |
| 79 | libs | 62c3cadc04f67ff1269282b0c82899dbaf6731826a32cac7bf55962724a22ab9 | 12384 | 66 | 2 | 0 | 3.0 | 5.0 |
| 80 | enron | 25f62d933399dea383bf62846979a22ecc8f1690e48346ebf651187aa90b96a4 | 13183 | 68 | 2 | 0 | 3.0 | 5.1 |
| 81 | enron | d148116f8c50885807c3d3d0844cd666e02cecf46587f2c20609989042e97a66 | 36814 | 125 | 2 | 0 | 3.1 | 5.1 |
| 82 | enron | a6c87c1954bf4fa14a8174885e35d1c5d525b922309ce695a2934f5b0fcdddef | 178902 | 3144 | 0 | 0 | 3.9 | 5.2 |
| 83 | enron | 73d6581934d66101ad248973690d80ac6bab5b022711f423b6cd18038f6c56a2 | 182894 | 3144 | 0 | 0 | 3.9 | 5.2 |
| 84 | enron | a4f9606d4767b5394a2f315b0c2608fe1bc1451f88f0a6ce6524dec9589a0e2a | 183865 | 3144 | 0 | 0 | 3.9 | 5.3 |
| 85 | enron | 995ae06d8695c7e034c7ac4f444130f5e729cbbb1cac891dc53f34a0bdcaa741 | 90298 | 39 | 1 | 0 | 3.1 | 5.4 |
| 86 | enron | 5f15a49e29e719856630ddc44934d51baf15a585a697aed0f9fb30c6cfcd8470 | 806598 | 19334 | 0 | 0 | 8.0 | 5.5 |
| 87 | libs | 24ee6b9018780f1159126e1c7ed7e987fbe719b91c0da9a7a23286aea1d391cd | 54880 | 1440 | 2 | 0 | 3.4 | 5.5 |
| 88 | enron | cb14fa328b83d516f605db6e3d739412d206a6320f887f56e81053e352cf2e97 | 73714 | 1436 | 2 | 0 | 3.4 | 5.6 |
| 89 | enron | 65c1522b9f2aa44535004a5386413bb16e05fbb338dbf5bf850b78059d4b4d0e | 736246 | 18151 | 4 | 0 | 7.7 | 5.7 |
| 90 | enron | 9a14a9af14e08c5be0b1e4bddc36b97166e9247c5ecc8f1d1cb32c02273d6faa | 69573 | 1715 | 1 | 0 | 3.4 | 5.8 |
| 91 | enron | ec551ffb072daed903c6af424510e949d8251df811acb08f49d0de13e22fdfca | 189708 | 3144 | 0 | 0 | 3.9 | 5.8 |
| 92 | enron | 03b6634eef54dc9f63b9566e911a3ba313208813725ca5a841012c47e8788d05 | 50040 | 2007 | 0 | 0 | 3.5 | 5.9 |
| 93 | enron | 94aaf2b59494f94050f52bab3d611b51c7f6d722fc8a3f1cc13e076c8a48592b | 166587 | 1294 | 0 | 0 | 3.5 | 6.0 |
| 94 | libs | 0c5c8dd675e69465e590b343089481030288fb1965d79cae19ce57a2ea238956 | 396312 | 27 | 2 | 0 | 3.6 | 6.0 |
| 95 | enron | 6eaefe7fc4e2fa2809208d665f1d06a7fd1af7dcdc5a2eb42e9e77daa71519ea | 278556 | 117 | 1 | 0 | 3.4 | 6.1 |
| 96 | enron | 1cc8ee773f75ce497e2f03589d58b4d35d46803708314cbf4e695d73319809f0 | 122797 | 6326 | 1 | 0 | 4.4 | 6.2 |
| 97 | enron | cd8bdc3d3e0ed89352882b89acd3e8d2cb0a4b606a4dad4ada498542b3b4abab | 139010 | 1522 | 1 | 0 | 3.5 | 6.2 |
| 98 | enron | 0450129791bc582b23066e78d841df9bbb749e31871f2ee56be39dccc3f2dfb3 | 335223 | 12676 | 0 | 0 | 6.0 | 6.3 |
| 99 | enron | 51feab5d9979c58b84b68c68f60d464c0948cb2728e7935f6816c6de6a2ba74a | 15689 | 35 | 1 | 0 | 3.0 | 6.4 |
| 100 | enron | 5dc72374e17cee2427cb064b4087055299b4470919d18f4c3e3bc338fd0a0ced | 207874 | 8897 | 2 | 0 | 5.1 | 6.5 |
| 101 | enron | 06f64ff7b93b7e14a6205e993f8ea0e358b3539f8c03b86f7ad62ebec7c11e03 | 93198 | 4674 | 2 | 0 | 4.1 | 6.5 |
| 102 | enron | 5adb945541f8895265a6fe0fe8c6e55d2cd7de61c9feac3820377b4f1fe8b2e8 | 29576 | 58 | 1 | 0 | 3.1 | 6.6 |
| 103 | enron | 9365b8d39a01461a0a66bf553df82b126d0a13e65567de334e74989d888ed5c1 | 63984 | 408 | 0 | 0 | 3.2 | 6.6 |
| 104 | enron | b399f221826007425ef24f3e3af8acbfc6d8c929a6d2295dd7270f8dc657302c | 620296 | 5484 | 0 | 0 | 5.0 | 6.7 |
| 105 | enron | 7c4d2d9ff7bd083ac9aa3a882853ebf8e0ac45125c257f63d8c9015ead4b3e64 | 156136 | 4004 | 1 | 0 | 4.0 | 6.8 |
| 106 | enron | 91e25b3fd39adbd75660255f868c2083ead2bee0594fe0905e1ae8ebd25feded | 927323 | 16144 | 0 | 0 | 7.6 | 6.9 |
| 107 | enron | 68c9020c77a689a8b4d0ca51b4b73273e58eea6fcee09baedb20f89cd004c821 | 166872 | 1294 | 0 | 0 | 3.5 | 7.0 |
| 108 | enron | 7c07e53f1b904516f85577ce05dec2618550db42110fe565cc6b11b15fbb9b8a | 153018 | 4314 | 0 | 0 | 4.1 | 7.0 |
| 109 | enron | d56fc679ee39e79b06cfb19254fa2e877811505f5d715108666c60dc23480642 | 225103 | 10064 | 0 | 0 | 5.3 | 7.1 |
| 110 | enron | 8f8eb8b37841d93e6d7ee295d2614b86abc06900b27550472a63c8f26fccdaba | 234642 | 13319 | 2 | 0 | 6.0 | 7.2 |
| 111 | libs | 4a5dcfb4fa3406cecf7bdb52d5794a0660ca691c9dfebe94f48818e6cc091d67 | 5213 | 4 | 1 | 0 | 3.0 | 7.3 |
| 112 | libs | eae1f20604d4c6889f7f8d8d0ddd926c1fbe71ebfe4c2189051be066de4417a9 | 24862 | 9 | 1 | 0 | 3.0 | 7.3 |
| 113 | enron | f35a04d26e2575ea221ba39686ccb8ce61488d180fe9ca132e52fba1e10ec457 | 24806 | 46 | 0 | 0 | 3.0 | 7.4 |
| 114 | enron | d40eb557eed7d6383d89f817cb727b1d30cf516ac6b8d7b409217ab5c80fc65d | 24790 | 48 | 0 | 0 | 3.0 | 7.4 |
| 115 | enron | 93f9dfb6772dec9d4cce3bdff1a2b3851d57c41862eb6a77f1e30fc9413960df | 42414 | 38 | 1 | 0 | 3.1 | 7.5 |
| 116 | enron | 36b11cc9b013ca2ead760911b68615e0348729228e01895c8d7886d603250d0e | 15727 | 235 | 1 | 0 | 3.1 | 7.5 |
| 117 | enron | b67c76dfccdcefb23f94968e70a246fc0f1f3ac1f19e87e03097acbf3034c177 | 29119 | 141 | 1 | 0 | 3.1 | 7.6 |
| 118 | libs | c224c784e77f321c59c90f6d93380ff74a0e0013f0346e4b2cdf0a5ff5e3840b | 47360 | 27 | 1 | 0 | 3.1 | 7.6 |
| 119 | enron | 7825a4675a04a9549dd9c941e7633b3da0e61da328c28e3e5bfc7daa048238c4 | 77058 | 2909 | 1 | 0 | 3.7 | 7.7 |
| 120 | enron | c72635a1cc77b8f028d33d11def8d886049e5b6a6083f4c7df86541c84fa6abc | 31765 | 422 | 1 | 0 | 3.1 | 7.7 |
| 121 | enron | 46913604772522bf36a3788a214e2b3b551baa4824f3b794d350198ac785df76 | 51462 | 294 | 1 | 0 | 3.1 | 7.8 |
| 122 | enron | da943c577fe6700d8e2bfff3ae131046107f46c8ddcb073bf8f618142fd915d6 | 54803 | 335 | 1 | 0 | 3.1 | 7.8 |
| 123 | enron | 6ac49e9e969eb3f78e2228cbe7ef07308b7b43ba385383ac577379be71283583 | 391213 | 4221 | 1 | 0 | 4.4 | 7.9 |
| 124 | enron | 0713c41a8ddba8be79566a89178cba2f0fccce99d05605734d6a1ce35d857b7c | 156870 | 2997 | 1 | 0 | 3.8 | 8.0 |
| 125 | enron | 1c341b73a4c6111bf6cf3280c179735b5840c120ad35e05d1638a30a0a38991e | 63324 | 1377 | 1 | 0 | 3.4 | 8.0 |
| 126 | enron | ff5a97ed767ec2263e069910b9185a79ccca90f33beffa65be51066645405838 | 147547 | 4307 | 1 | 0 | 4.1 | 8.1 |
| 127 | enron | 3a71c28eadfd6c22f0c2e90393e0eb582d595493c2468f83be153d60109d0053 | 278139 | 10548 | 2 | 0 | 5.5 | 8.2 |
| 128 | enron | 5b30791667803397939b03305e44f97329ca001d310aea7ed5bd9a41f38f9001 | 69650 | 1715 | 0 | 0 | 3.4 | 8.2 |
| 129 | enron | 6e6cb9bd8c4c85f86bb06e3545815c6c61230b88e33930ee07bd06020f4bc936 | 76971 | 1692 | 0 | 0 | 3.4 | 8.3 |
| 130 | enron | 496c4cb4aec032f35ed93eea667ccf44123dcf5aacf3c0071e498c7365bd9702 | 77260 | 1692 | 0 | 0 | 3.4 | 8.4 |
| 131 | enron | abcd498082df74a655a83505bed3cc67645d8b951e07753906289f2562c7657d | 61334 | 1850 | 0 | 0 | 3.5 | 8.4 |
| 132 | enron | 52e2f23f6c1bc18222ad7c023d4dccd0f74f8b15f16c9d6df0830b44fe9d28df | 135971 | 4887 | 1 | 0 | 4.2 | 8.5 |
| 133 | enron | 058fa275137df42e3865c750a1fc4afe9ba8b0db0da364514686929c7c0cffd5 | 150431 | 1334 | 1 | 0 | 3.5 | 8.5 |
| 134 | enron | a3c1a729730391f3646e20e61d327d5e4212320cbcedc676a08d3bfc61f79481 | 172855 | 1294 | 0 | 0 | 3.5 | 8.6 |
| 135 | enron | e75118be0759c215322a8854a8d0eafc6aca58791efcafdecf0b190385eccaf3 | 153503 | 1551 | 1 | 0 | 3.5 | 8.7 |
| 136 | enron | 0e3af65c458c03349b52e87699fad8e7b432a34879d2bde3c0f1d519246628f1 | 617895 | 5524 | 0 | 0 | 5.0 | 8.7 |
| 137 | enron | 10816085fed515a7954d4bb570b55e3364869066e1a6323f6c72edb53b88106c | 285133 | 846 | 1 | 0 | 3.6 | 8.8 |
| 138 | enron | 9b0d8bbb70c4bcfff27832faffaca5531946b2782bb0c65c57ac8b174312f65c | 56742 | 2885 | 1 | 0 | 3.7 | 8.9 |
| 139 | libs | 27153dbb64dafe5a8cb7007671169e78d0d23bdcb7d34234b1e2146c825496cf | 4971 | 1 | 1 | 0 | 3.0 | 8.9 |
| 140 | libs | 480bf24d597289d18bf96485e97d92e188ea3effa51598f75b9d59ae6058fcb7 | 5541 | 1 | 1 | 0 | 3.0 | 9.0 |
| 141 | libs | b6f47da53efa98e56fcb2ca76a2c246cff2dcbafe245672ffa7cbab95a169068 | 5795 | 1 | 1 | 0 | 3.0 | 9.0 |
| 142 | libs | 15cc9f6a1f08c631f3dd097e88c2f889ef7a8772c3625de58b594e7ddc932467 | 5971 | 4 | 1 | 0 | 3.0 | 9.1 |
| 143 | libs | c94d6be35d4b4e9ef2fb92757a6d67bde4308fc57b4372f803efb24b84a6a5f8 | 6446 | 1 | 1 | 0 | 3.0 | 9.1 |
| 144 | libs | dfa6d0e1b6a9983f34344bacdde63a0ba93a5c794b1e454efe181f6b04074711 | 6686 | 1 | 1 | 0 | 3.0 | 9.2 |
| 145 | libs | 8c11bcd48ca97ac96a086df242ee3d5d822402fa011331e4133ff7081a3d2564 | 5288 | 15 | 1 | 0 | 3.0 | 9.2 |
| 146 | libs | 28024006f012cf72c7ef24e90dac30900aac0dbc5200361be24dc13f5564e36e | 6501 | 10 | 1 | 0 | 3.0 | 9.3 |
| 147 | libs | b66a0be6a6dd60a06c8b107a82c90859f4a30ad773ade1aa915e11438bd0c97e | 4703 | 23 | 1 | 0 | 3.0 | 9.3 |
| 148 | libs | c519f7c90b74d2c92500c47370a6b85a0a19430a0ee6470254fcbeaaa71fc505 | 7928 | 3 | 1 | 0 | 3.0 | 9.4 |
| 149 | libs | 27d4006ff328210a7fc06841e427fd6ab40eee63e935311f1f0b08a534cc2fd7 | 7772 | 6 | 1 | 0 | 3.0 | 9.4 |
| 150 | libs | 05ba49682aa679b8615c8be301227a6321146651c5655bc7c759df932dd4da96 | 8763 | 1 | 1 | 0 | 3.0 | 9.5 |
| 151 | libs | 865eb0645b095f56e1057ecc25673b91604040f3e4ef362d56504ba8ffc02639 | 8846 | 1 | 1 | 0 | 3.0 | 9.5 |
| 152 | libs | e546faf310a765e7b4dd7bdb28c20846f9dd3123e109b2e287229e2f00315381 | 7535 | 12 | 1 | 0 | 3.0 | 9.6 |
| 153 | libs | c3e08940b17d4f0e1d2d278e26a4c59c4f4d66370ce6306aa930f31006473982 | 9151 | 1 | 1 | 0 | 3.0 | 9.6 |
| 154 | libs | 1c85fed929eeb836fc53f746ebbf554d9d4aa2e3b3a28754920d2fb32903e452 | 8901 | 7 | 1 | 0 | 3.0 | 9.7 |
| 155 | libs | e89fb8a826a64aefd457b225d8fe875d26ecc5c08f979707cc7d7b92b4cac80d | 10650 | 2 | 1 | 0 | 3.0 | 9.7 |
| 156 | libs | 48293a9961176b34a6cb0f8a44c4e76dcafccab241b64d12a6446827e5380cee | 11204 | 2 | 1 | 0 | 3.0 | 9.8 |
| 157 | enron | 18b6b8a2b92e95778d37f9dcbdd7e9cad2b36f46146dd297e39aa90414c34c01 | 12236 | 1 | 1 | 0 | 3.0 | 9.8 |
| 158 | libs | d18aade6ad10435baa4be3ac1c4ba4c15e48248bc24b91926920f3dec0aca033 | 10981 | 10 | 1 | 0 | 3.0 | 9.9 |
| 159 | enron | 3e98939a53d89ff5a8066caa9c483cdda5996ad920ca510e1239d1ada1ed00f5 | 12278 | 4 | 1 | 0 | 3.0 | 9.9 |
| 160 | libs | 34f2fe682aee3c00dfcff7f05f5120450166f9338f41c72f2e0098a8d2acd0cd | 9770 | 22 | 1 | 0 | 3.0 | 10.0 |
| 161 | enron | 3f8263800d1e34636d7f8753fc96fcda16c3c433d6932ae1c09e90bd8f9cbfb8 | 11994 | 11 | 1 | 0 | 3.0 | 10.0 |
| 162 | enron | 3290f2a52feaf8703c35c397b8c03857244d8fc1b9c0e7778c8ec97065acd0ee | 13067 | 5 | 1 | 0 | 3.0 | 10.1 |
| 163 | enron | dd608df0f3049b4ab9d3d727b43a2d45a67bc819f0133199bf953c1f920afc97 | 12731 | 11 | 1 | 0 | 3.0 | 10.1 |
| 164 | enron | 102bf570c06b9e522a8f7c0b79b46a4ab329b7e51ec94956fdf9338e19bbae82 | 11822 | 20 | 1 | 0 | 3.0 | 10.2 |
| 165 | libs | 2d9e138925ca26660f12189009782e1cd7531b634e509e8a1c960dc7cc1c1180 | 12999 | 14 | 1 | 0 | 3.0 | 10.2 |
| 166 | enron | 3eec92eafd8e7f596f8a1300bdfdb0349732c6f218ac29dc647996e60f9c15ca | 10694 | 33 | 1 | 0 | 3.0 | 10.3 |
| 167 | enron | 06b7d3b484fb265dbc6a6a906978be49e38a69ca85b73d99f98f5c94bc31872e | 12977 | 21 | 1 | 0 | 3.0 | 10.3 |
| 168 | enron | 54bc4162f7f6e94d1d957cb5840456e288e6d5108ed6205aae05b816dd660cbf | 15604 | 4 | 1 | 0 | 3.0 | 10.4 |
| 169 | enron | b3d103a6f8baa31ab83f2a7eec8a5daaa8a687b5aabd74713b526e6e682c8fc6 | 11618 | 41 | 1 | 0 | 3.0 | 10.4 |
| 170 | enron | 7e22a5b04be8b01d44ba1b4a57471b125b12a4a17a6a070f92807a0b9adf252d | 12390 | 37 | 1 | 0 | 3.0 | 10.5 |
| 171 | enron | 52957e440a027a4cea256d32774c7c3b5ed495f4795efb2731be5595af8ccbb5 | 15208 | 24 | 1 | 0 | 3.0 | 10.5 |
| 172 | libs | 187d6ca93dd8c063f7a15c5b4f62e368254a727b731faa9a844941dfa87d1dab | 17614 | 13 | 1 | 0 | 3.0 | 10.6 |
| 173 | enron | 17fe778bd3ac8f583020e4244500d35119304fe27bf5a8f812525506af3309a9 | 15717 | 34 | 1 | 0 | 3.0 | 10.6 |
| 174 | enron | b116c83134e9c3779d5a33a7826d9760c5913d82cab5693be730f63d2ffe75c1 | 14884 | 48 | 1 | 0 | 3.0 | 10.7 |
| 175 | enron | 83bf4edae483b3ba8b1556043bd72e4277d92eeab6fd8daabb382a90eae4e42b | 19804 | 23 | 1 | 0 | 3.0 | 10.7 |
| 176 | libs | 95c54e12bc9a7a80191fe1c9e2df866f029913d4b0cf8136fc0f5a65a63758c6 | 12428 | 83 | 1 | 0 | 3.0 | 10.8 |
| 177 | enron | 7234fe4fb0e911427b42d4909f79e2b8e804b3ecc5442fdc8634e49e00dead35 | 23580 | 4 | 1 | 0 | 3.0 | 10.8 |
| 178 | enron | 46450937aa1ebd5b5fb784d884c4ed5bcd50ed0905080866e256e8f2cbfce202 | 13310 | 87 | 1 | 0 | 3.0 | 10.9 |
| 179 | enron | 1a13e46803e245393ea7f5666fb0f17a215021d17d19ba3444721034a9fe4ac7 | 23526 | 14 | 1 | 0 | 3.0 | 10.9 |
| 180 | libs | c44565119d7375af84fa6741c0029bd3edf706ec8626a256d0f95170bc18030b | 20493 | 36 | 1 | 0 | 3.0 | 11.0 |
| 181 | enron | cfbfd9fffc68023516dd51c537cb7560676d4db763557bb40bbe966d71c583ae | 24535 | 23 | 1 | 0 | 3.0 | 11.0 |
| 182 | enron | 72d239c78f98f6c22ed9e968107cd1de9276a5b85d5a185915897d402faf0d11 | 17820 | 81 | 1 | 0 | 3.0 | 11.1 |
| 183 | enron | 95c5b19b78a70f9341f18bb7dde851b129287643ac2afd15b6745fb0e8ae6790 | 24933 | 50 | 0 | 0 | 3.0 | 11.1 |
| 184 | enron | 33a396cc8cacd36ae5232fbda77f66479744605480f027f1992061000dd7b7fc | 24829 | 51 | 0 | 0 | 3.0 | 11.2 |
| 185 | libs | 1b9040fc0ed15665fe10e154462abdf0b5a286d6a0b9243f05107fb1176680f9 | 32626 | 3 | 1 | 0 | 3.0 | 11.2 |
| 186 | enron | 198fdfefc9772d0c23bbb2e6520b1fcb23c7774e87cb940cdae391b9bee9131a | 30356 | 28 | 1 | 0 | 3.0 | 11.3 |
| 187 | libs | 68c74a61143d82a2cce0f1c23b00993a2a2287c31a9b8ca97bef51f544acef43 | 32268 | 168 | 1 | 0 | 3.1 | 11.3 |
| 188 | enron | 6ded4703593e2c573f36a01a459256b02f2ece3cb9b9256888b9b90e50f48a05 | 52010 | 36 | 1 | 0 | 3.1 | 11.4 |
| 189 | enron | 30b3e5e97432f5675ceeaa5d4730c0dfd4b900db279139471da0ec69cf1bf276 | 20507 | 270 | 1 | 0 | 3.1 | 11.4 |
| 190 | enron | 428b9894f754149ca66f3a54e433f427098d8537a1efb668ed9463c032b37713 | 53796 | 38 | 1 | 0 | 3.1 | 11.5 |
| 191 | enron | 850032d95d2cef760f9bba2218edc93139dac6984b2511842049aa2d3bb6ba22 | 17989 | 316 | 1 | 0 | 3.1 | 11.5 |
| 192 | enron | 684ac1cd402a75b24ab70e86b9a32d92347a5ec48b694b02c25a5beda1a25ff1 | 26921 | 318 | 1 | 0 | 3.1 | 11.6 |
| 193 | enron | 67d474e64d837c62fb56fab3b62afefb4b1bf6d8867b37ab19e577f3bb458811 | 26503 | 339 | 1 | 0 | 3.1 | 11.6 |
| 194 | enron | 238ec90d406508cd3a947ca3e7608ef4ca50e2fb2920dab623bf9eb8737dd030 | 34799 | 371 | 1 | 0 | 3.1 | 11.7 |
| 195 | enron | 90308fa8e80032ec0632616f668a98593f621f2ab2f9b58db398394d9961009a | 45853 | 363 | 1 | 0 | 3.1 | 11.8 |
| 196 | enron | b5a63003b3d31a30a5d39ee41fbb45145f8700eae848762a3f7b2869171e3f34 | 86902 | 84 | 1 | 0 | 3.1 | 11.8 |
| 197 | enron | 5e578435af2d7dde29458be0b7668e9054e2b0790b004a37db74d604415b849d | 35702 | 507 | 1 | 0 | 3.2 | 11.9 |
| 198 | enron | 51e44fb59e8877fc77ae5993cccb5acc1a53017fbedc3f820d2a2600dbf97e2e | 59543 | 339 | 0 | 0 | 3.2 | 11.9 |
| 199 | enron | b5178d0df59765fcc8ea7f46c4d830cb9d1411a78ffc26b27442081cee37ebff | 59722 | 339 | 0 | 0 | 3.2 | 12.0 |
| 200 | enron | 2efc3e8b47faf634f93361d55b0dc231e393e70fa3dde5e77af3a438f48e3660 | 59868 | 339 | 0 | 0 | 3.2 | 12.0 |
| 201 | enron | 5a8e79632caa152c95d97694360335d2a05216a54e1a720f1c3bb9d78a5d61e9 | 49073 | 487 | 1 | 0 | 3.2 | 12.1 |
| 202 | enron | f4afba210e6688acf8f0b3704928bd48ddf08d4e8dc485b32998a2dab34a0d5c | 113125 | 85 | 0 | 0 | 3.2 | 12.1 |
| 203 | enron | 9fafd14edbc6a3e3a0df14698dd010238e70120c6cc48b80106155b61492e3f8 | 35194 | 831 | 1 | 0 | 3.2 | 12.2 |
| 204 | enron | 59735e921172fb7c964fa08ec99e40ba3ea95bb086834580f221fe05c13dbbc9 | 139423 | 104 | 1 | 0 | 3.2 | 12.2 |
| 205 | enron | 15318d2cc490c127055d231e4a6a28e2575a2703fa84eeeca80a2313ace09456 | 27370 | 1122 | 1 | 0 | 3.3 | 12.3 |
| 206 | enron | 3a2d80bade5c1d0883337ae3e32d4ff84e4232453eae2e5aea990966d5a29243 | 236755 | 7842 | 1 | 0 | 4.9 | 12.4 |
| 207 | enron | a2823a69a1b7cdadbda31645c569329af25e9177cb5785c6e2aae33b21262a49 | 65719 | 894 | 0 | 0 | 3.3 | 12.4 |
| 208 | enron | c9ed0297a5013f7788d25728ff76c47631df7374933cff60a0e1de1fea362e9e | 172143 | 4245 | 1 | 0 | 4.1 | 12.5 |
| 209 | enron | e6763df70a5d23f3c5df0834ab8eceb6f05e3f79301bd530c81059ed926b8cb5 | 111958 | 4716 | 1 | 0 | 4.1 | 12.6 |
| 210 | enron | bbddbcaec21fed2d330eb906380dc1f6610a81cfe62260f72995d0086b26b41c | 99458 | 845 | 1 | 0 | 3.3 | 12.6 |
| 211 | enron | 796acefd367cee723ebb2db966c70783e33c78943f27408807822374b39bff61 | 98015 | 4995 | 1 | 0 | 4.1 | 12.7 |
| 212 | enron | 7a9a64b2e77ebac66e4f7047f4eabb24a54b9d01475a35551be286580e8c310f | 154919 | 4594 | 1 | 0 | 4.1 | 12.7 |
| 213 | enron | 409c88ccc8d067282002786f16c410923f89cc435123c4b6d63fd8f6884d2af5 | 41009 | 1509 | 0 | 0 | 3.4 | 12.8 |
| 214 | enron | 47e4886c6f3a9806d611f2d376ef6665da444e105ef48b4d5f29a2ad356705e1 | 279916 | 4022 | 1 | 0 | 4.2 | 12.9 |
| 215 | enron | b5920acb57b284e00ef01a578b362953ec56534f5bc0a2d8c380038266092b46 | 41133 | 1655 | 1 | 0 | 3.4 | 12.9 |
| 216 | enron | d734f06d818c223acbce4ef7beea5b10a94382517dfedf10de7e03ae9cc31e12 | 895832 | 17145 | 0 | 0 | 7.7 | 13.1 |
| 217 | enron | 581d5a6d4316c5bf25962405424864c8daeb8e0b13efcbea03831db722572115 | 74236 | 2230 | 1 | 0 | 3.6 | 13.1 |
| 218 | enron | bef6dfc253112496928a6b5d51e47d8389240707d8ee4d05e6f0be477e7b219f | 152001 | 2152 | 1 | 0 | 3.6 | 13.2 |
| 219 | enron | 2ac247502f5e90842368b1a9f11d242b52b4280e73bfb521330f88aefc3c065f | 221798 | 1933 | 0 | 0 | 3.7 | 13.2 |
| 220 | enron | b53ebc422afa1395e595cc582d1c3ff3f0c060772f4cfcdcfb23cad13790fc7c | 119278 | 2768 | 1 | 0 | 3.7 | 13.3 |
| 221 | libs | c718268285da63929de80fd1d619d50b4de898dcee75e531272e76fdc71c9a38 | 187295 | 2303 | 1 | 0 | 3.7 | 13.4 |
| 222 | enron | dfda98a1918a310d226909541f87c955969c85bd4a1762bc80cf7638abdc4355 | 895969 | 17145 | 0 | 0 | 7.7 | 13.5 |
| 223 | enron | 376eb1e0bd8e0fb3a5ea8144e3d3c71e4aaf7b37dc0d753bd8a46465bb4c9774 | 159492 | 3175 | 1 | 0 | 3.9 | 13.6 |
| 224 | enron | 94eda7d547734d1158afd55793f8f12f94739f9893b3f821e90d8bfd466d19cc | 629837 | 5524 | 0 | 0 | 5.0 | 13.6 |
| 225 | enron | 1fc87c05c680104d7562aba1a4e1c4a25c018dc67769ce8476b252e8b580491f | 121261 | 4496 | 1 | 0 | 4.1 | 13.7 |
| 226 | enron | a276fe4f795da52dcb73f6655cfbcc530dee0a66efff9e44a494cc5f446e6fdf | 223497 | 8976 | 1 | 0 | 5.1 | 13.8 |
| 227 | enron | 72fc731fdb8271aa0be8b4e596d26fd6c04474693e10dae7f3e4abb5b18869e3 | 66916 | 482 | 0 | 0 | 3.2 | 13.8 |
| 228 | enron | a09c069d02fddb2a9d61c04ec915b0f379a43e2bd2aeed609f6f43ffbeaafe88 | 181868 | 5085 | 1 | 0 | 4.3 | 13.9 |
| 229 | enron | c865b1608e24bdd6bbdce837a04c48c50635f6d71c08840cfc43a4ee5ca78f7a | 6013714 | 54690 | 4 | 0 | 22.5 | 14.3 |
| 230 | enron | a596d91115d6d219fcffe8c296a76ad6a31f536feb73851ad1c8aaf1350467ad | 66605 | 669 | 0 | 0 | 3.2 | 14.3 |
| 231 | enron | ff3af284a47ef2221f3e784afb2c331f888435d5c6da50eb3a27824fc371f2ad | 68286 | 722 | 0 | 0 | 3.2 | 14.4 |
| 232 | enron | 68a24f612ff5d3e57e963bb03a818f4e0c25072a412a7ef6d096afe95717bb0e | 582433 | 3810 | 1 | 0 | 4.6 | 14.5 |
| 233 | enron | e612ffacd50f5156103877b895482e86371a6991472d8551a0b7d02da07e027b | 204622 | 6644 | 1 | 0 | 4.6 | 14.6 |
| 234 | enron | a2d15b9eb7e28c9c8d0022eb66a6ec96969110771db9c3592703cdddc0a969b0 | 2278500 | 26576 | 2 | 0 | 11.6 | 14.7 |
| 235 | enron | 3134d7664b5fafd593d319ec98e97ae864f6f090a7262a3f05a5d82c6f49afd9 | 766653 | 9056 | 1 | 0 | 5.9 | 14.8 |
| 236 | enron | 04149b88948559c1f2d87a74b2a46fac8e2432ea3e72fc7fe5ec125897eea27f | 97186 | 2240 | 0 | 0 | 3.6 | 14.9 |
| 237 | enron | 2fa5ba29d0f214742974997f8990de594ae8d582fdb0927be9dcc95a137f8af1 | 97085 | 2443 | 0 | 0 | 3.6 | 15.0 |
| 238 | enron | 69afa38a39410974b8ee1c8b7af6f15933f1282fbfcaa1abea3529e8fe8fa4ed | 356014 | 14042 | 1 | 0 | 6.3 | 15.1 |
| 239 | enron | 1db68e8219d239bfbbff2ec6925acd4ecefbaf1fb2fdb67d757aa909bec046f4 | 239502 | 2375 | 0 | 0 | 3.8 | 15.1 |
| 240 | enron | b12066a20c47ed59e10aea598bebfb008f51e5a6fd3bb15b35b8570783227ef8 | 1647725 | 5169 | 1 | 0 | 6.4 | 15.2 |
| 241 | enron | 1f4cccf2015b092c5273eda0933404bf31c31d31f16f965c82deafca23b17693 | 81530 | 3760 | 0 | 0 | 3.9 | 15.3 |
| 242 | enron | b2f471b456a95b7250efe4ed7cddb329f3c4c893153877cfebb3084d3e3db911 | 79913 | 3817 | 0 | 0 | 3.9 | 15.4 |
| 243 | enron | 812b738864e3b0e2b03d95c7c7a1acc1593a3b9ace5bd9eb1146537f7f5a0ccb | 80685 | 3834 | 0 | 0 | 3.9 | 15.4 |
| 244 | enron | 24fbec6a2b632e87159ad462f1a935070d792bc97060b6b3e3a0e430bee0fc50 | 261614 | 2652 | 0 | 0 | 3.9 | 15.5 |
| 245 | enron | ec67409d4758582326f1f187b7108258ee1eb57e4537431fc07a363800ecbdef | 167091 | 3646 | 0 | 0 | 4.0 | 15.6 |
| 246 | enron | c06aa8f1d2569369db31614cd718ccebec858cbe870a6f2fe7cf0104ab39eb82 | 234405 | 9940 | 1 | 0 | 5.3 | 15.7 |
| 247 | enron | 9883289e9a824b2730ab914f180a54c08c993dcea2629c712c00a5c08cac8c70 | 314122 | 9496 | 0 | 0 | 5.3 | 15.7 |
| 248 | enron | c50f44a4fcb076724c9c5ec02b1a07073afa10a07d5f7f43a8aaaed7947f0191 | 88596 | 4567 | 0 | 0 | 4.0 | 15.8 |
| 249 | enron | 178d94d47c3d37836b20385f03c252fe37f7391ac853fc5a2e7186f6f6d933a4 | 206753 | 10907 | 0 | 0 | 5.5 | 15.9 |
| 250 | enron | 2e93f9798698d79818b6a206968f1b8b0f5ca0ef87783aa2d22eedc685e658eb | 864206 | 23082 | 1 | 0 | 8.9 | 16.1 |
| 251 | enron | 2d1daf137a8af05c14547b25ab540e2343dacbe7c0692c894874e0558ea17dc3 | 122981 | 6327 | 0 | 0 | 4.4 | 16.1 |
| 252 | enron | 5880e56633acb3633117b0cf81b49415c179b58700bad852f8c0fc3b96685e4e | 122730 | 6337 | 0 | 0 | 4.4 | 16.2 |
| 253 | libs | adcf40d7ea5b7fafd4917855805883c2a32d8db1636be074582b5cf21ea51574 | 8882 | 6 | 0 | 0 | 3.0 | 16.3 |
| 254 | libs | b6db1aaf280f1bc11809a4b9aaf1dc7b2d1c0fd8b2f40d1d1d14982c5610cfa9 | 13284 | 12 | 0 | 0 | 3.0 | 16.3 |
| 255 | libs | acb265e34c8d6ea4f8e7f51ed2cfa9571656c15ddecd4300dd784745674703c5 | 17987 | 18 | 0 | 0 | 3.0 | 16.4 |
| 256 | enron | a6116b408f03fc198ab73f4ac51f30639f8e5f8646f66cf77091a6e71703d78c | 13622 | 68 | 0 | 0 | 3.0 | 16.4 |
| 257 | enron | 0f9fc4d1b3ea60004da2410d5d74b0d36f09f81ea07d4c650f723cb14fdb794e | 15825 | 54 | 0 | 0 | 3.0 | 16.5 |
| 258 | enron | 4e5e0642344dd4c4bee282a907f6086a7667c323393eb133b28e15c593689d7c | 16468 | 70 | 0 | 0 | 3.0 | 16.5 |
| 259 | enron | 8d6ca1071a07e950213f64bc3054238ca590d44dc05b9d27eb10160a7b857e8e | 16563 | 70 | 0 | 0 | 3.0 | 16.6 |
| 260 | enron | 59093c5541bb044a3b235b621136b5b97260b144054eb54a69d94547c16da164 | 13031 | 180 | 0 | 0 | 3.1 | 16.6 |
| 261 | enron | db99ba101d85dc550fd0dc471170382705d149aac410fc81dd7d98897e00ca42 | 165661 | 6748 | 0 | 0 | 4.6 | 16.7 |
| 262 | enron | e60460448e549f17c386b0347e3a7b4cb493e892c52b36d384d396126f0ee3f7 | 20563 | 153 | 0 | 0 | 3.1 | 16.7 |
| 263 | enron | acbc393587c04df5689457676c3df131ca534e38c953fc24443abc628efe0f67 | 20758 | 153 | 0 | 0 | 3.1 | 16.8 |
| 264 | enron | 42c68a72afcfade84d03c1b38b08858820e746f354c6709ac7430f0dd349e908 | 16203 | 225 | 0 | 0 | 3.1 | 16.8 |
| 265 | enron | 05764d7031d6efea8a0acd8b9d5bf88d221517bc6bd185159ebc9d7ce7775024 | 31766 | 422 | 0 | 0 | 3.1 | 16.9 |
| 266 | enron | 5e863b54cc46a81b817c8365ad328ca6d9ac1a2ab1bae73d2f2942efcad9fada | 31768 | 422 | 0 | 0 | 3.1 | 16.9 |
| 267 | enron | 8fcad31a27777ba32f3d3121624ebce3802aa8426f03d58b532db45bb20c6bda | 150865 | 7437 | 0 | 0 | 4.7 | 17.0 |
| 268 | enron | d4952e34b34884e33d830ec2466f38b85711c0ee4b11e7eef34ced60efc3614e | 150873 | 7437 | 0 | 0 | 4.7 | 17.1 |
| 269 | enron | 7099055607b850db64661064dcaf2a2bcc6970a4325e29b89f343df4617b7209 | 49052 | 417 | 0 | 0 | 3.2 | 17.1 |
| 270 | enron | 8b20da0f66524366091b00fa4fe820c10d1fbbd1cf0c81bcda31af9065e72c48 | 53465 | 436 | 0 | 0 | 3.2 | 17.2 |
| 271 | enron | 7f469429084b87372573310619461d2e759b02d7d1e97969e3aa4033e59f2569 | 46485 | 541 | 0 | 0 | 3.2 | 17.3 |
| 272 | enron | e558cece2343810901d5e8111bfd7f7d9eed6131423dad028eff27690de92fef | 85768 | 372 | 0 | 0 | 3.2 | 17.3 |
| 273 | enron | 64524213db829878469def1f8e1588db1a78da924702c5256dcd9049d2b46b47 | 71745 | 547 | 0 | 0 | 3.2 | 17.4 |
| 274 | enron | 6fadca7fbe0c75bbadacf21dbab201f62636f87069a5231dddf0292ba5408b84 | 73680 | 609 | 0 | 0 | 3.2 | 17.4 |
| 275 | enron | d919ba41ace1f52fae68854896dbad5a591463250d2eeb1cc5ccf141731c049e | 73699 | 609 | 0 | 0 | 3.2 | 17.5 |
| 276 | enron | bc8a363ca00aab3ccf4fb48066fec2d73c2ee61ebf2a7b681c0ac989b782cbea | 72989 | 709 | 0 | 0 | 3.2 | 17.5 |
| 277 | enron | bda7d64a0f9eee06e7e850dd6f80c466b1268ff0f42c346321ee88b9da2a18cb | 62323 | 802 | 0 | 0 | 3.2 | 17.6 |
| 278 | enron | 5719307655589a509fe79b61c7c297ab03fd49f92a8a50df1ae25932fe660dc2 | 226395 | 232 | 0 | 0 | 3.4 | 17.6 |
| 279 | enron | ec2c4379e702f654c9583dfc51c9b8a396acf1c36c5eeb1173217652802adf79 | 119721 | 1035 | 0 | 0 | 3.4 | 17.7 |
| 280 | enron | a997a5294044e202e2aaba033c0fc03940c377b9f6cfcb3e98096b70e1e089dc | 183208 | 1058 | 0 | 0 | 3.5 | 17.7 |
| 281 | enron | 1431d41c6d10e6303c3c86d6eb50f5d55c91da946851a3e95eea1348bb64926f | 97915 | 1827 | 0 | 0 | 3.5 | 17.8 |
| 282 | enron | ee6deb6c034cc34ff1f7cc12e1f250dbd09cd88f130a2953f346b6f8e7659a14 | 118549 | 1973 | 0 | 0 | 3.6 | 17.9 |
| 283 | enron | c75751efe902812cb8629624a3abec66243664e65258ea2336d289425a401667 | 75208 | 2411 | 0 | 0 | 3.6 | 17.9 |
| 284 | enron | e97d16a2cb269b819af3a1c2e2ead93927551268f4508ef1b5aab07f0ef4290d | 82585 | 2360 | 0 | 0 | 3.6 | 18.0 |
| 285 | enron | be5c86fe8464951676c9eb42980b17243ec902f4be355322fadd347168182634 | 82945 | 2360 | 0 | 0 | 3.6 | 18.0 |
| 286 | enron | fecf735fe375ede03c7baac671de08e9a6b92971a00f07013b346dc49ee7c945 | 83029 | 2360 | 0 | 0 | 3.6 | 18.1 |
| 287 | enron | b853022fad9c0b6ff1665335902427cf42ae928045d836b44c04fa3d57726e73 | 83294 | 2360 | 0 | 0 | 3.6 | 18.2 |
| 288 | enron | a0794c00c7e86fba8048de2017b314dffa2bcb5b9faa8597d246361aa40258eb | 76520 | 2600 | 0 | 0 | 3.6 | 18.2 |
| 289 | enron | c2f73ece863cc87016dad5b7d9eac5b2b713260b179e49681561a5c2084ad144 | 3232473 | 108691 | 3 | 0 | 29.4 | 18.7 |
| 290 | enron | f86dffc7cc189f5db584b94f91428cfb3d59a7d9035cc6b8419bba302981f870 | 147688 | 2371 | 0 | 0 | 3.7 | 18.8 |
| 291 | enron | 53457e53614b2edbe0221e35c347690d25a36e4743c9d7e4991032b8c41168cd | 148168 | 2371 | 0 | 0 | 3.7 | 18.8 |
| 292 | enron | effdc6ab49e7b63248aac8899f0df3cf9175fe3ab7f72ec69c1234576a354a6f | 148377 | 2371 | 0 | 0 | 3.7 | 18.9 |
| 293 | enron | 3d7fa0700c477abeed78728236c31a7f5ef31713bc4135b457993fb9fc35b11a | 78809 | 3249 | 0 | 0 | 3.8 | 19.0 |
| 294 | enron | 23f1109e1631afbfc77af3b2699b69eee9056642f505813881ce054372221ee5 | 79360 | 3249 | 0 | 0 | 3.8 | 19.0 |
| 295 | enron | 807fadf201f58e56411cb0564c45207ef52a41e0729bfa461ce244dfda3a453d | 655231 | 28720 | 1 | 0 | 9.7 | 19.2 |
| 296 | enron | 8e45eb900533a487422f5d5c5c0f36e04b0d9d3fbc858007544f2487a3a8785e | 87231 | 3800 | 0 | 0 | 3.9 | 19.3 |
| 297 | enron | 758823b637b61f168fdf513f21ded5d32c6d0bf243cad9d5243217c6bd8d7bc5 | 140053 | 3441 | 0 | 0 | 3.9 | 19.3 |
| 298 | enron | 1e2dcde8e9b57b68ca4e0dbd2f926b3c1ca1d0a34009f92ebc6da19533aeeb9b | 262002 | 2652 | 0 | 0 | 3.9 | 19.4 |
| 299 | enron | f9c703e884f9f741a4fa73ad2a01f887385c9707fe07a4a83e93082e03e4128e | 125117 | 3841 | 0 | 0 | 3.9 | 19.4 |
| 300 | enron | ddb528df707d24f83c4849f35c5f2acf1ce818befc0bc8a367e3a2b8d19f22b4 | 184027 | 3556 | 0 | 0 | 4.0 | 19.5 |
| 301 | enron | 8dffff450081a2393f1b8f4affeb8c2b766293d5ea407290378230f5c51fb4c2 | 102991 | 4547 | 0 | 0 | 4.1 | 19.6 |
| 302 | enron | 89a1cd0972bc441879d11d04fb1a2785e953feb40456c4a900bda86c38be4a6c | 361133 | 24594 | 1 | 0 | 8.4 | 19.7 |
| 303 | enron | 80e3e0b2ed288173bbce855362d2bc48acc3acb40b04e8e33aaaa291336400b8 | 788364 | 1052 | 0 | 0 | 4.3 | 19.8 |
| 304 | enron | 112bbb47fc194f1ad0b0ca34f3002b05193f9da37caee0ce68afb9a83ce4f860 | 791263 | 1124 | 0 | 0 | 4.4 | 19.9 |
| 305 | enron | 4531cf7a3219bf5b4733327f54673f5cf009287656f23a0fd24cf5a0d0191a35 | 1127959 | 21038 | 1 | 0 | 8.8 | 20.0 |
| 306 | enron | 24b139a58e00c49dee2af3cc8c210de827fe48ffe33e7e3305f98a48a84c3fbb | 150616 | 6792 | 0 | 0 | 4.6 | 20.1 |
| 307 | enron | ec6aff855552914aec9df777c68ba5efb9e0f2702a0cbdd3e8bbe4743f7fb4fa | 1278993 | 83081 | 2 | 0 | 21.4 | 20.4 |
| 308 | enron | 77676dd7069393dc0451a5f80920c8b2a83f55767d9c4610f0e5a77eed84ae4f | 146078 | 7837 | 0 | 0 | 4.8 | 20.5 |
| 309 | enron | c7d75e8dd1715471cbc703b07b514a62ecf112aa1b486073dd99bcab329486af | 372950 | 7146 | 0 | 0 | 5.0 | 20.6 |
| 310 | enron | fead0e99e02d3a61938e4436cc2e92287478c7582bc20bef3d442d591795d0b7 | 6067517 | 55554 | 0 | 0 | 22.8 | 21.0 |
| 311 | enron | 51f018d66458eef22d27b25cfb3984a60c523ae0500b45e8ef57d75fa92d60be | 896028 | 17145 | 0 | 0 | 7.7 | 21.1 |
| 312 | enron | 56689d38fefa0278f43388665d61201452cc2610897a7eee1c98fc3b63010bc9 | 6273325 | 59010 | 0 | 0 | 23.8 | 21.5 |
| 313 | enron | 880ff4a2f07d7848ea208a37f85bbfc8a46b161da17a6b77a74b2c09281c6f8b | 734729 | 21499 | 0 | 0 | 8.4 | 21.7 |
| 314 | libs | 7ca51ffa10eec327b39290711202fa4e625f9b45ad5d9cc77390677283c2fd0a | 7308 | 4 | 0 | 0 | 3.0 | 21.7 |
| 315 | libs | 772a42443e7bd30f51a1926374b2aff8b0bb1c0a5dd356e2c852adb1bf8fc106 | 7751 | 2 | 0 | 0 | 3.0 | 21.8 |
| 316 | libs | 02e5f8146eb0df22564cc88337406a6f7fcf9acddf82931955ae6828e4d40326 | 8037 | 1 | 0 | 0 | 3.0 | 21.8 |
| 317 | libs | 7a8aee1debda9fea93e787d165228aaead0288799e4bee5befd21b2065d336f6 | 8579 | 1 | 0 | 0 | 3.0 | 21.9 |
| 318 | libs | 5a6d13188f8f500053c25233147ac1deef397cfcb126c108c491022902c44cc1 | 8849 | 1 | 0 | 0 | 3.0 | 21.9 |
| 319 | libs | aef8bab8e5ef9a9a9165aef0ff34fa6ca4ed626668ecb1145d8e1c337da7952b | 8926 | 2 | 0 | 0 | 3.0 | 22.0 |
| 320 | libs | 28a2ed967d41887a32a2862c66b8db31f28b16109e0a02477570c90ad516fc5b | 9181 | 1 | 0 | 0 | 3.0 | 22.0 |
| 321 | libs | d257f64763257f03a89e19d8968d9dd967339118d2d93cb1e69439250d10609d | 9188 | 1 | 0 | 0 | 3.0 | 22.1 |
| 322 | libs | f5d4282bece8a5427edf976866fc93eb65f16120540abf21635102d2bb2d099c | 9230 | 1 | 0 | 0 | 3.0 | 22.1 |
| 323 | libs | d8ebab6e0a48a5fc3b227555a46fe7205aac174029e70e7f92573c34b91d6d0f | 9128 | 3 | 0 | 0 | 3.0 | 22.2 |
| 324 | libs | c1e17e7d0d8b8f1cae95461eb9145e025a9a151efc52718f72596283b5da20b0 | 9964 | 6 | 0 | 0 | 3.0 | 22.2 |
| 325 | libs | 87d624ad9e4493a4481ec84d14336757085088f9cfca382b89b4c0d9d1cb98d1 | 9743 | 8 | 0 | 0 | 3.0 | 22.3 |
| 326 | enron | 483bad9a2b149b2e6b0f24c653c6ec44f999bbc645b6299d9b5dd4107e0353be | 13293 | 5 | 0 | 0 | 3.0 | 22.3 |
| 327 | enron | f82adb6eec07df28cc21bd4e604092be575374b8e9d19581c0f937a369c890d8 | 13300 | 5 | 0 | 0 | 3.0 | 22.4 |
| 328 | libs | f41bbd13cb3f9648b2492bc7ffce4c0544a27d0c62daec5c670ac15bdfe04bbc | 13571 | 5 | 0 | 0 | 3.0 | 22.4 |
| 329 | enron | 6ef56fa83ea600f82969377760215e47c49de8d87c0ffbf6ee90dadd1178e582 | 12733 | 11 | 0 | 0 | 3.0 | 22.5 |
| 330 | enron | a6d1a391036bf5f2e6acf041e7b620277afa221264cace78822b0e76a9452934 | 12416 | 14 | 0 | 0 | 3.0 | 22.5 |
| 331 | enron | 1a8e4a24cd317c6c936c6ae8e02adf8e6c0ae7b0d79d6453f832c60dedbe4e76 | 12413 | 15 | 0 | 0 | 3.0 | 22.6 |
| 332 | enron | 16234623e1c05dc0bf9aff755bbd3d1157a5e7837529a28be08abeb1bf8bd3f7 | 13805 | 6 | 0 | 0 | 3.0 | 22.6 |
| 333 | enron | 6dc15e810b8401908719454ccd3f8964f78d6261080219c50ef21ecb5023fe07 | 14368 | 2 | 0 | 0 | 3.0 | 22.7 |
| 334 | enron | 805fd9d31833a433cfef9c5b2c35e14e4ccbdc47572fe3612a302657a589041f | 11613 | 22 | 0 | 0 | 3.0 | 22.7 |
| 335 | libs | f4169bab78cf219c9619de70e22f79c28a383a76debe8dba096c2637449daa8e | 15414 | 1 | 0 | 0 | 3.0 | 22.8 |
| 336 | enron | c4f8302527d0268bd16539ea73dbb1fce8eee02fca64a4ce48f9834d07c7d40c | 14446 | 10 | 0 | 0 | 3.0 | 22.8 |
| 337 | enron | 8613d321c267078cfbb4fa7d51d6bd602359f4829c4f29d799c069440fe79736 | 11967 | 30 | 0 | 0 | 3.0 | 22.9 |
| 338 | enron | f5ba52e3eccce389338ff520fb440eda2d4c6071a3cc883b81e5b6da10b0090f | 13032 | 23 | 0 | 0 | 3.0 | 22.9 |
| 339 | enron | e7e6bcff8b8df85aa1e85776e40f7ca1f4ee7e9ec8832632d56ac1d589ca8354 | 12278 | 30 | 0 | 0 | 3.0 | 23.0 |
| 340 | libs | e5eee015cd0de3def4b4a0d16cc3c68cfc340249fd830ed92b1c093ba53a38c3 | 9915 | 49 | 0 | 0 | 3.0 | 23.0 |
| 341 | enron | 63b4775bba7e86c0b2ac0aaca4b4ef07ff33f2f7225c976084bee78d712c6b6d | 12415 | 33 | 0 | 0 | 3.0 | 23.1 |
| 342 | libs | 50d4f7d5d0ffe17e2251b21cfb66bd5018e3b1791c7f5c27592a47a9d9e004b1 | 16892 | 1 | 0 | 0 | 3.0 | 23.1 |
| 343 | enron | 775ea1252968a05cbdd4933a98ab147304d4539c00a9efe164badd6a4f310bba | 12421 | 33 | 0 | 0 | 3.0 | 23.2 |
| 344 | enron | 3b5da668a1f5d5d811b43e038c8809bfe2fcb1f6316d18157c18d7ab3515fcd3 | 12885 | 30 | 0 | 0 | 3.0 | 23.2 |
| 345 | enron | c73ac7bbccd2b5bf20bf0f9b97f1832cbd01da4f286fd68b874748151dc1e72f | 12752 | 40 | 0 | 0 | 3.0 | 23.3 |
| 346 | libs | 16445dc0deba00671ee4315015a970d415ce9886da59c974532d946e99276c66 | 12394 | 45 | 0 | 0 | 3.0 | 23.3 |
| 347 | enron | 23746978796538d809386cb52b59ea74c2cc2a0148f4d97f16baab9daa942b2f | 17193 | 12 | 0 | 0 | 3.0 | 23.4 |
| 348 | enron | 2ed6be408e67cb60c516e7f5ce51e88680f584e5b77f5616c79188093a7e2581 | 17242 | 14 | 0 | 0 | 3.0 | 23.4 |
| 349 | libs | 96afa10c8f1f633fc2eaebf21a0883c96a8be2cc5f38fc8c2e826b5d9c140d4f | 14577 | 38 | 0 | 0 | 3.0 | 23.5 |
| 350 | enron | eaa156e7537b537e74d46f933bb5dd5d3b57aaf72c22813f217ee1761bfdf7f0 | 14893 | 48 | 0 | 0 | 3.0 | 23.5 |
| 351 | enron | 4eab4371ff1f103348ee9f72d9eaf401553a6ec4c4ac8a5dad8e8f7b530f5539 | 12963 | 67 | 0 | 0 | 3.0 | 23.6 |
| 352 | enron | 28818f9cd20f90ac06809c1d6747b332e6fbfa47b6d04ec3a760e1879abb5550 | 13062 | 77 | 0 | 0 | 3.0 | 23.6 |
| 353 | enron | 7791c6e3fc193119d433e741185717977d9ef8e6666c58b28692414850cf0de9 | 15496 | 75 | 0 | 0 | 3.0 | 23.7 |
| 354 | libs | 0915acf26f4bf9484b649702fa3544123cc802bf4efda7ebeb82c2759cacf281 | 25314 | 7 | 0 | 0 | 3.0 | 23.7 |
| 355 | enron | 0897717d5050d833f858aab1c8dbe29b6f7a2ee9f39675efe10dadf7a632b60b | 23886 | 24 | 0 | 0 | 3.0 | 23.8 |
| 356 | enron | 5b29ad1195873084e901cc0a89d69a20990451d43e849325575dbec72395937d | 24428 | 23 | 0 | 0 | 3.0 | 23.8 |
| 357 | enron | 85e4eb9486b29b3e2acdb015581dda8f3997f709c8cf7536f2c1f25f33f0b782 | 24431 | 23 | 0 | 0 | 3.0 | 23.9 |
| 358 | enron | 5c706a8f203e81ace2db3d502354e4a77519e43b570bf3821c0ef3703c3d4817 | 24442 | 23 | 0 | 0 | 3.0 | 23.9 |
| 359 | enron | 3ca9639d42bfcefe08992cde310884fd50ac1d54957b4e915649c1119029ee67 | 24047 | 27 | 0 | 0 | 3.0 | 24.0 |
| 360 | enron | 5394d274517a06c081ae41bac42e1cf991216c0096823cbde2636c4dc41ae978 | 19706 | 59 | 0 | 0 | 3.0 | 24.0 |
| 361 | enron | 78845df636a72956223786be2128251d3c16f7a2ec863973c58db9285dec4746 | 22438 | 46 | 0 | 0 | 3.0 | 24.1 |
| 362 | enron | de4edb01b4d900187e2cc883ef6a5a40bbc4644b755204ca6b25592661cd1538 | 24966 | 29 | 0 | 0 | 3.0 | 24.1 |
| 363 | enron | 0a41f70a25ae5a1d4d6b84057ba7e980ca721dfefb8a506b4045d5780f8d4823 | 17890 | 81 | 0 | 0 | 3.0 | 24.2 |
| 364 | enron | f5bd0274ad88862383ffec7b8965021568dc48d4f3c038886f175c99f7104a33 | 18003 | 81 | 0 | 0 | 3.0 | 24.2 |
| 365 | enron | d56e88de153e7e507934fed854e310c3497e49e1db6cfda20e3368184efb612a | 18054 | 81 | 0 | 0 | 3.0 | 24.3 |
| 366 | enron | c2b63db866423f1385d3d3e30001b54d9c7c4a069ca974f9dcf44f41834d121e | 19250 | 81 | 0 | 0 | 3.0 | 24.3 |
| 367 | enron | cf3d2df7cc0bec8130485be2428e41e14131454a337e45c3254149bbaba6a7ea | 14071 | 137 | 0 | 0 | 3.0 | 24.4 |
| 368 | enron | 926b00ea2e43b50570359b576af1749e80a890982d4b250cd9c9be68d1484c6c | 30064 | 31 | 0 | 0 | 3.0 | 24.4 |
| 369 | enron | 172e95898b3143c30aea3d6f68d709d50d0839362165cd581246526f44cc8d8d | 33935 | 27 | 0 | 0 | 3.1 | 24.5 |
| 370 | enron | 994063dcbe34c662edb9e0705f80a4775627138f14053bc400db1512f0292fca | 23760 | 114 | 0 | 0 | 3.1 | 24.5 |
| 371 | enron | 4a2c89691cfdd9272d9383fd755afc160608dad80b067b97c3e25a348c9833cc | 23768 | 114 | 0 | 0 | 3.1 | 24.6 |
| 372 | libs | e2d4b686c10ba04c8659acab50fdba198b298ea754cb4ff897ba2d8e63ab10fa | 39918 | 19 | 0 | 0 | 3.1 | 24.6 |
| 373 | enron | a926ed62365cf45a3b047bba4b1a0f883932c249056dabdcf6ed7078113f8e82 | 31945 | 98 | 0 | 0 | 3.1 | 24.7 |
| 374 | enron | 3a649f8580ee8a0fc31d75be5e69bf3f7ad688b2120d79783826c6108000d881 | 16757 | 209 | 0 | 0 | 3.1 | 24.7 |
| 375 | libs | 607ff06ff0295d0920d81b27e09b91e86301b9f3862d1d22b093f8f8c5a4de28 | 36134 | 72 | 0 | 0 | 3.1 | 24.8 |
| 376 | enron | ea0aceb8a0774c5d70a3817dd6bd1d344b8a43961cc954503f9ba57c653e541b | 42021 | 45 | 0 | 0 | 3.1 | 24.8 |
| 377 | enron | afbc54a877ea9930113702e65cdf5e053f417c4781036221491c360d1a633268 | 19961 | 208 | 0 | 0 | 3.1 | 24.9 |
| 378 | enron | 68ad201941a898be45cae5f035c32e4112eac1db3bd66d55b589ddabb9f6e973 | 49489 | 3 | 0 | 0 | 3.1 | 24.9 |
| 379 | enron | 9121d9461b7407c08ade13997f32a386f0db8549335240508c3d04c724270c5a | 50047 | 3 | 0 | 0 | 3.1 | 25.0 |
| 380 | enron | 99f0cb0d9699f16821701b69833bd39b645c42e2a26e7115e21f88db74dfc6d6 | 50389 | 3 | 0 | 0 | 3.1 | 25.0 |
| 381 | libs | c8a86eedc7da9b9f421210cd42bb1164df4d068f39c1b10768f8e780f4f58852 | 46589 | 51 | 0 | 0 | 3.1 | 25.1 |
| 382 | enron | 6682fda9db98cb7003c13c6d2b8818562989b74c68f58c8c499e386068d3d303 | 19995 | 253 | 0 | 0 | 3.1 | 25.1 |
| 383 | enron | d9fd90621cfaa9ba1fa50f997f1f292a1dc95afa57f2c9f685afbac37be35e60 | 26325 | 208 | 0 | 0 | 3.1 | 25.2 |
| 384 | enron | e14069d8de7bc72880237759bc3c157a7b639e65a0214ec0fca04d9190b4b945 | 18111 | 316 | 0 | 0 | 3.1 | 25.2 |
| 385 | enron | 68c7e2ca3e7f2b25fd2f49e6d39f327c3e60c5c8cf9b35630756e56347a84c49 | 23016 | 293 | 0 | 0 | 3.1 | 25.3 |
| 386 | enron | cd8100f07a606077dc3ac6fdb43e132e1b655f9ce50f800cfa6d31d9beaa3d82 | 3021270 | 118312 | 1 | 0 | 31.0 | 25.8 |
| 387 | enron | ff1a08d615aac18689311097588c5590536c2c8ef66e2974c27db8419c009072 | 27020 | 318 | 0 | 0 | 3.1 | 25.9 |
| 388 | enron | 04b5001c4968cf1940106fc190223f33c4c8bfa04ecbbe2fb033ff77a9b42f0a | 36311 | 288 | 0 | 0 | 3.1 | 25.9 |
| 389 | enron | 5da2d91461d2fb5b33f4a5fbd20e3194d70b74964f267ef22233d79a89fbbac8 | 64233 | 111 | 0 | 0 | 3.1 | 26.0 |
| 390 | enron | f9f2e22f173a40cee0895b2969d00c74bd1735906cc144131d935c022293cb2d | 54228 | 185 | 0 | 0 | 3.1 | 26.0 |
| 391 | enron | 709fa817876c6bdcbf0f762191e2100b75eb6dd20bd7eb36fbb195b48d30edbd | 54228 | 185 | 0 | 0 | 3.1 | 26.1 |
| 392 | enron | bfc406c28cc80bd8d92aa1aba2f5d0dac1ff8aa31a6b01a3f29f13f13f47a55a | 50918 | 216 | 0 | 0 | 3.1 | 26.1 |
| 393 | enron | e04a258280deccd14af1465fe0b73ed82ce13a624ea0c59f10015de428bb0fa1 | 50957 | 216 | 0 | 0 | 3.1 | 26.2 |
| 394 | enron | 865e87d56ad5f9db8373c3b915cbada1ffa1b452b0be3f176bdb3b2072e6c75e | 21985 | 436 | 0 | 0 | 3.1 | 26.2 |
| 395 | enron | 41703cc8b6aa31449fea8d7ed5b62c38d07f9f9e6ba4f4c95e1d13b19008bdfe | 22947 | 431 | 0 | 0 | 3.1 | 26.3 |
| 396 | enron | 661dd56deb6914b921e9d9da9229a43aa4c046839bea7e5bc4208296be890030 | 37551 | 345 | 0 | 0 | 3.1 | 26.3 |
| 397 | enron | 234dc0b1312c7650c757b02b941db94b19b23b0ece4fc558d6f94c8602cf90de | 34808 | 371 | 0 | 0 | 3.1 | 26.4 |
| 398 | enron | 5c26afb931a930eb135d7f108813158fa4d6b9880f6369b5b1d4317dcd4de0f5 | 34862 | 371 | 0 | 0 | 3.1 | 26.4 |
| 399 | enron | 12d2f4f97a8effb021d6c231729e007ca9a30aa26a465852bacff15edede7ffb | 35388 | 380 | 0 | 0 | 3.1 | 26.5 |
| 400 | enron | f11297d3e35c87b9070bf5b2b261656a799ffbfa02f1853588006967480ecca3 | 35444 | 382 | 0 | 0 | 3.1 | 26.5 |
| 401 | enron | 3827c453991636167872ca0958e87e77ff376037b13026ef72027243f1cf9b12 | 65872 | 179 | 0 | 0 | 3.1 | 26.6 |
| 402 | enron | e812cf4d13957f83c6a54d31da44864a04e30ab9d3e8e9ffc08083a8f2302709 | 66931 | 185 | 0 | 0 | 3.1 | 26.6 |
| 403 | enron | 01f23116a93b98ee363b928768805b667a80d816126f25acd31cada11c2b5e2f | 45864 | 363 | 0 | 0 | 3.1 | 26.7 |
| 404 | enron | e7d3b0535c79fc782bf60159676b6727f725b9d9a6ae40d1c604e638c241387e | 37878 | 431 | 0 | 0 | 3.1 | 26.7 |
| 405 | enron | 72c06f2f08cd1d2e5374a61019469f1575258bd4438dfe23c47800d235bcb88e | 43136 | 412 | 0 | 0 | 3.1 | 26.8 |
| 406 | enron | ca4d294d080da4d282130adaefa759fe828f59fc829511f876d08a40a116d98e | 27147 | 550 | 0 | 0 | 3.1 | 26.9 |
| 407 | enron | 93984f4d78d09bb831c149658720b04d34a8cfa8ae495ab2806d3184881e3114 | 35910 | 507 | 0 | 0 | 3.2 | 26.9 |
| 408 | enron | af9e50fb5404ddd9f905eb2f4062e3e6ee71bd51be88fd6bb2d423a32655581e | 35989 | 507 | 0 | 0 | 3.2 | 27.0 |
| 409 | enron | 3e334f7f0f38538c53791961b9cba17ec99ec19d178013d6eb215eaeed887d55 | 36136 | 507 | 0 | 0 | 3.2 | 27.0 |
| 410 | enron | ff231e33894d2c7673b77cf0b9a5dab23304c7402aecd627f18fef4219519cc8 | 36238 | 507 | 0 | 0 | 3.2 | 27.1 |
| 411 | enron | 3836e51a819ed91b9df014b9103e59ba7b9b1469405a251bf61ba3b09c34d207 | 60049 | 388 | 0 | 0 | 3.2 | 27.1 |
| 412 | enron | f14a2d7094331446d89e91c333272808e260bb008bec5760d082b8cd5af41583 | 35839 | 633 | 0 | 0 | 3.2 | 27.2 |
| 413 | enron | 80517ece20c7ef99800249cf7efc6610e971721a984f729c3c8a6773478b8e81 | 29058 | 685 | 0 | 0 | 3.2 | 27.2 |
| 414 | enron | ae41d0e0215072178fe9e8ca0ebcd41df66788a16397c7f0b680f6292cb8419f | 65430 | 463 | 0 | 0 | 3.2 | 27.3 |
| 415 | enron | a2d0bf00ecdf721b8b933b48ab57149b9ffc445fe34802b3ffd7274d9fc87d0a | 65668 | 465 | 0 | 0 | 3.2 | 27.3 |
| 416 | enron | f28f1331f9eb58755968e25d595f6ab91c5195e7d6f383e3e03ac7ad7bc4e388 | 65832 | 465 | 0 | 0 | 3.2 | 27.4 |
| 417 | enron | 7fe62c7357ffdf71f2d14460fd05a7a77cae0de4f5b4338b090b0dd12b5f1fa6 | 21816 | 783 | 0 | 0 | 3.2 | 27.4 |
| 418 | enron | 25f91308002beec9e8b21e6f9e8f3aa17eed1127fdab2c9e03d9d9eca905d94f | 35216 | 831 | 0 | 0 | 3.2 | 27.5 |
| 419 | enron | d13fd5cf4dfb107d7c40f7289c34cb16d58cedfc504f767ad14cdc687d5685f9 | 60933 | 730 | 0 | 0 | 3.2 | 27.5 |
| 420 | enron | 0d9874136795f6c56291738c43a6947412238fa2f08f3e27f102ab8167a935f7 | 61899 | 756 | 0 | 0 | 3.2 | 27.6 |
| 421 | enron | 2188a7d2f8658710917130ed222e0ad6f08d3badcd63f5d39a94010fed5c79a0 | 28566 | 1122 | 0 | 0 | 3.3 | 27.7 |
| 422 | enron | 92e7a925167d7d83d365dc63760d6e39e7129146075c6c2c8d74b271855baeed | 50304 | 1005 | 0 | 0 | 3.3 | 27.7 |
| 423 | enron | 80adc68b7349c1beb5a3bd81cf71cefeca383228c350360c9accb99a3eb10326 | 65986 | 930 | 0 | 0 | 3.3 | 27.8 |
| 424 | enron | fc929ea4daf963d97ccfdd94a9a0957fea340575fdac5c08fa9b162ac38a3c21 | 61190 | 1082 | 0 | 0 | 3.3 | 27.8 |
| 425 | enron | 59c2cf2b19c3ddb46df317ca4765f474364c429b142c91c89f40ce8fdb706eb8 | 49813 | 1202 | 0 | 0 | 3.3 | 27.9 |
| 426 | enron | f2e1bd8fa09e1a5d27b0544a9fb7c916e58da4f53b192fae353e430f311d2b4f | 85252 | 967 | 0 | 0 | 3.3 | 27.9 |
| 427 | enron | d1798a6c134c2b3a5860ad9919114a5a5c2669c0bb6fa4dd283a5375eb88224b | 864912 | 28561 | 0 | 0 | 9.9 | 28.1 |
| 428 | enron | 06661a63392982745c0f091f5c624b009b6906b1d17f6c17b8abe15ff1723210 | 79632 | 1022 | 0 | 0 | 3.3 | 28.1 |
| 429 | enron | 9c6cb50c2c2055b0ca6bca79b2c7270f22e668430b75bcc32c472a3cbc313a8c | 79595 | 1028 | 0 | 0 | 3.3 | 28.2 |
| 430 | enron | 117a241e5b6d84295ca19059fef49c8e5f7d9191487414f6ed6bdf5ae94c772d | 188731 | 299 | 0 | 0 | 3.3 | 28.3 |
| 431 | enron | dc403c7aedc18022b0f349d08bbf03f7c36b535dbeac63e40f046d15de335584 | 100635 | 989 | 0 | 0 | 3.3 | 28.3 |
| 432 | enron | 85d0ca7b2580a4daa915c260dde3e2c7c910b7ddd9d8b809f6963fe481a9538e | 100637 | 989 | 0 | 0 | 3.3 | 28.4 |
| 433 | enron | 17fc3d89e1f114bc26c10333047107c65125ce0330cb7b135cbc62251a8e525f | 107513 | 1075 | 0 | 0 | 3.4 | 28.4 |
| 434 | enron | 39b09965d14581f9bc5b00f545b1fcea5685f3162b04207570c2c5d60b0fad22 | 227247 | 240 | 0 | 0 | 3.4 | 28.5 |
| 435 | enron | c98e206fdc4b64d2fde7f2a7f3af350fb7bdd2d31f03fc80320c44f588a412db | 67466 | 1389 | 0 | 0 | 3.4 | 28.5 |
| 436 | enron | f63bf719df5e682c7a6dd07a2d9d72f13bd6c8393ab781f387cd8e31f795756a | 74498 | 1339 | 0 | 0 | 3.4 | 28.6 |
| 437 | enron | e0695f559bf826cd2933ea491126bf9e22906d8dca61fd5beab1be879b81f1ce | 260928 | 64 | 0 | 0 | 3.4 | 28.7 |
| 438 | enron | 339ecc4c749cd31182f44007a8c6f1ea21473a0ebb3166d8f26d705bc0ca96eb | 51594 | 1839 | 0 | 0 | 3.4 | 28.7 |
| 439 | enron | 1f6ab66acde64a1147218bdabd49d52d3ca9f0018fe4a35b2f68297492d75bd4 | 52147 | 1847 | 0 | 0 | 3.4 | 28.8 |
| 440 | enron | 632fcc13c716009a5d9ef51b6d7e7d0a6cba64c3c92d9075f7bf73d309676def | 77488 | 1715 | 0 | 0 | 3.5 | 28.8 |
| 441 | enron | e4ac63b694f24104f7e043f0d43cb02c83b41f316acbd06304b6bc14a09ab433 | 49038 | 2182 | 0 | 0 | 3.5 | 28.9 |
| 442 | enron | cee38040010d667d52940c600b7a6f9e3a23cd1f684a192f949a6e20bc43b4ad | 49163 | 2182 | 0 | 0 | 3.5 | 28.9 |
| 443 | enron | 7335a09ce4cc1fe46110b45416f1865b46c179aa988b8fdc8397d6ff83bbb6a6 | 49700 | 2189 | 0 | 0 | 3.5 | 29.0 |
| 444 | enron | 5f081425b686d9a00cadbb06346164b1d2e848044c94cba447ca0833a84846ae | 74360 | 2230 | 0 | 0 | 3.6 | 29.1 |
| 445 | enron | 730c97562d5634e12bd82c8986a671a217f8a2263c032d4b6c415c2d7025f1b2 | 74480 | 2230 | 0 | 0 | 3.6 | 29.1 |
| 446 | enron | 2da6618e88094ff3e4873e669a65aa77c74a75935fa844cc393d33db86a70265 | 75509 | 2230 | 0 | 0 | 3.6 | 29.2 |
| 447 | enron | c5420df0fef697a56018ec103699e2f73b22fd5f9a5540d377873e9a048ebba0 | 125667 | 2223 | 0 | 0 | 3.6 | 29.2 |
| 448 | enron | 9eae99da0578bb95d2a8ae14b671f67ecfec72a23e3c8a45f42505b9816e43be | 74481 | 2727 | 0 | 0 | 3.7 | 29.3 |
| 449 | enron | bf6bca08466ef7a94d483f2655772fbc8dbb6fe516827a38d3275a7f685782a8 | 186679 | 1928 | 0 | 0 | 3.7 | 29.4 |
| 450 | enron | d1bd8f07049bd3bd9c693a97bbb3cfa9e680aab73adccc9bedfb91304e87e5a6 | 398335 | 846 | 0 | 0 | 3.7 | 29.4 |
| 451 | enron | d197af09dff9ea236a00354a65cb2788c96a3c158855080af51bc9c2f35d6343 | 471688 | 19365 | 0 | 0 | 7.5 | 29.5 |
| 452 | enron | 2c73ffa84355a233c9142372e2ebb212a3494ffac5e30237324c5a09b1d4cd27 | 137573 | 3798 | 0 | 0 | 4.0 | 29.6 |
| 453 | enron | 97ebf569dc5ec61294f374ce3ccb30420236f6006625435ab6e20a6f9af7f857 | 154500 | 3698 | 0 | 0 | 4.0 | 29.7 |
| 454 | enron | f61db9e59a92583944b38538d4e72e9e0dab5e303063a5e92e8a9dc9dc617e4c | 160461 | 3879 | 0 | 0 | 4.0 | 29.7 |
| 455 | libs | 8dfa997a642c00a1da8c0750c371f204553a98c537ca046028b052f9b424a074 | 654688 | 352 | 0 | 0 | 4.0 | 29.8 |
| 456 | enron | 2f9a4fd20b8d114bbd669d0e0514434f2fa4509eaee7764614d12020e59e492a | 667952 | 281 | 0 | 0 | 4.0 | 29.9 |
| 457 | enron | 4e712e95fa91c850e8f6fe88b46f6f891aa453782bf7defc3e891faaec24198c | 367427 | 2488 | 0 | 0 | 4.0 | 29.9 |
| 458 | enron | c6a22da373eb305b4cf99e2720b4a9c524d5c2ea5f5bc6c8f81ffd1c6d9fd3fe | 156187 | 4004 | 0 | 0 | 4.0 | 30.0 |
| 459 | enron | 44a96f0341fa7704f6326cbe515b819cab54c1ca4cdd38b7c6b37b32f64c5a9c | 156457 | 4004 | 0 | 0 | 4.0 | 30.1 |
| 460 | enron | d6626f7a9a2203f69ef30a3eb99f33a98b2329c89e73490ba66767de75aaef7e | 371218 | 2489 | 0 | 0 | 4.0 | 30.1 |
| 461 | enron | d5aaf59b57e7fa6b4dd385e2f39cb04bddc762fdd766237f3636e1c5c88ad1c8 | 69264 | 5281 | 0 | 0 | 4.2 | 30.2 |
| 462 | enron | ce0fd0bde81eaec31825b032e597431694b458ef18f2d7fb336d30a972c2562b | 122883 | 4922 | 0 | 0 | 4.2 | 30.3 |
| 463 | enron | efc15c9ea026ce153fa1ff483cd014dd728daf1d6996a706dac9d0e83ccbcc6c | 123093 | 4971 | 0 | 0 | 4.2 | 30.4 |
| 464 | enron | dfc1ed47f84cd107877526a2e7ad2589278b981a225c7bc490f5b6a1f0503d0f | 135593 | 4964 | 0 | 0 | 4.2 | 30.4 |
| 465 | enron | 771f969e462da79c1dd535967a35eb5e5c8e0fde06aa13acd13fea843967d0db | 351995 | 3644 | 0 | 0 | 4.2 | 30.5 |
| 466 | enron | f6dd1dfea3cef09a3e7e349a8ce644a64e2b85f0cde1ac50949404109d8f5039 | 352017 | 3644 | 0 | 0 | 4.2 | 30.6 |
| 467 | enron | ca76dd2d19eae7016c7fc247e723f67703120fa0155ef3ed3d46fa0d63016287 | 182015 | 5090 | 0 | 0 | 4.3 | 30.6 |
| 468 | enron | 51730eff70122ffed40901cce0f4040ec52bde0af6c2a3188c9f3ed33c6c5e75 | 182654 | 5093 | 0 | 0 | 4.3 | 30.7 |
| 469 | enron | af39135053b7b11085cb39de5e526553803021c915df2111a9fc117efa085118 | 182917 | 5102 | 0 | 0 | 4.3 | 30.8 |
| 470 | enron | a031a23de9de54282408c17c4ac094a1f543db34118c325713004c3ad4f3bcd8 | 916055 | 946 | 0 | 0 | 4.5 | 30.9 |
| 471 | enron | 3369f95a2471e294865b933efbb02f3d65f794dc1090e08b41c5a19956c36f79 | 187803 | 7404 | 0 | 0 | 4.7 | 30.9 |
| 472 | enron | a49d912ecebe8b645bc54a8755fe6c69b69f20646fa58c2a9a4299e994405b98 | 187282 | 7692 | 0 | 0 | 4.8 | 31.0 |
| 473 | enron | 430bfe7f5afe973c6d0e5e04b4c4f7b905306ee34b1890c7fd65c14b60046876 | 237009 | 7841 | 0 | 0 | 4.9 | 31.1 |
| 474 | enron | 992c5dcb68770306a85624507b04d4c516c4662ce1c97c19750b5957c20c385d | 203032 | 8870 | 0 | 0 | 5.1 | 31.2 |
| 475 | enron | 31155e0d82fe897160f1502832c83992bf60caadc03557d0bb67e6f2c8e98cb6 | 584438 | 6273 | 0 | 0 | 5.1 | 31.3 |
| 476 | enron | 0a423da10f18aae85b31f2e3005ecf2756c9e30f19a4f298bd653346096252a7 | 169767 | 9275 | 0 | 0 | 5.1 | 31.4 |
| 477 | enron | 84f29eaed0bf32ac1ba003dd81d287dc0b76bbbbe8b130932763a04baf329a06 | 167954 | 9370 | 0 | 0 | 5.1 | 31.4 |
| 478 | enron | 06433a82fd99ee2f46bbf1752afa0a8fdd4e903f5e2d06aff4c56762f0413c8e | 539981 | 7679 | 0 | 0 | 5.3 | 31.5 |
| 479 | enron | 7dc529d7a9a15a81fb491a14b3760d6a508f940ce002423d91dcacd68fa061b3 | 540374 | 7760 | 0 | 0 | 5.3 | 31.6 |
| 480 | enron | e7058210fba453d4d3455c4eed457008909640e55f91288aa93a579a73c4ddec | 1523246 | 2150 | 0 | 0 | 5.6 | 31.7 |
| 481 | enron | 96a8b721e75bf98431fc1494d0ab2273324250cc2641a5d5c1ad03e6bb5167dd | 1650710 | 5431 | 0 | 0 | 6.4 | 31.8 |
| 482 | enron | b084e88d656555f2a5ce99623ade77a1469e5aff9eb68361213629af966777b9 | 605354 | 14339 | 0 | 0 | 6.7 | 31.9 |
| 483 | enron | 2fa63a0866309b5ee680717acc8c5edb9454d7da5936de10e53c4a05ef45ec2e | 2410473 | 114690 | 1 | 0 | 29.4 | 32.4 |
| 484 | enron | 67ba6156adda35d917c932f87846fd2df2f3d3eaa37018ae078c022bd612f502 | 1334149 | 12445 | 0 | 0 | 7.4 | 32.5 |
| 485 | enron | 740373b73ae2a1913dbb1e1f034ba48e7971ab522f656f426adc98b6cad59323 | 990889 | 39090 | 0 | 0 | 12.2 | 32.7 |
| 486 | enron | 1e20aa43bc8c1ceff60220c531f2ae8fe5150f6d48d3ccf39107cdb3b661f143 | 1251313 | 46139 | 0 | 0 | 14.0 | 33.0 |
