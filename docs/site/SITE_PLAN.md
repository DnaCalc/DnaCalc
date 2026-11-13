# dnacalc.com: site plan (dc-6q2k.1)

Draft for the owner, 2026-11-13. Nothing is built before the owner agrees (dc-6q2k.2 builds it).
Figures were read from the repository and tracker on 2026-11-13; each names its source.

## 1. Who the site is for

1. **Excel users on Windows** who want a second spreadsheet that opens their workbooks, runs their VBA
   and loads their XLL add-ins. The top audience (week plan goal 1).
2. **Add-in and VBA developers** (Excel-DNA, xlOil and C API authors; people with .xlam libraries)
   who want a place to test functions outside Excel. OneCalc is their bench.
3. **People who check workbooks**: auditors, model reviewers, CI pipelines. They need the Linux runner
   and a "same answer as Excel" record.
4. **Developers who embed a calculation engine**: the crates (not yet published).
5. **The curious**: the handbook, the playground pages, the blog and the videos.

## 2. What is true today (the source for every claim)

| Feature | Status | Evidence |
|---|---|---|
| Formula results equal Excel on the oracle corpus | 77,036 of 78,264 formula cells (98.4%), 62 workbooks; fail = mismatched + refused | commit 6847cc02, 2026-11-13. Last split (74052f7a, at 76,903): 497 mismatched, 668 refused, 1 missing |
| Function bank | 4,607,949 of 4,729,476 Excel answers matched; 501 functions in the full bank | commit 5ed48847; docs/plan/bank-coverage-2026-11-12.md |
| VBA in a workbook | dna-simple 109 of 109 on module, JIT and .xlsm routes; dna-examples .xlsm 22 of 26 (4 refused), project route 24 of 26 | crates/calc-vba/tests/corpus/baseline.json (`windows` section empty) |
| Many third-party VBA projects | cases recorded, Excel answers missing (vba-regex 69, arraytools 51, stringbuilder 35 cases) | same file |
| XLL add-ins (Windows) | OneCalc page acceptance m4: 350 of 350 matched; xloil-utils-sql 12 of 12 | docs/plan/WINDOWS_CROSS_BUILD.md, second run |
| .xlam add-ins and .xlsm macros on the desktop | simple-xlam and simple-xlsm 109 of 109, dnaexamples-xlam 22 of 22, dnaexamples-xlsm 23 of 23 | same |
| DNA Calc page cases (opened .xlsm, add-ins from the page) | 729 of 732 match Excel | commit 887ebb5e (dc-hsa5.12, dc-0f1k.23) |
| Add-ins see the real workbook and its events | not yet: the desktop passes "Book1", no SheetChange or AfterCalculate | dc-1nbp.3, open |
| XLL hosting on Linux or in the browser | no: Windows x64 only | crates/calc-xll-host/src/lib.rs `compile_error!` |
| Desktop apps | Windows only, built on Linux with cargo-xwin; zip folders | apps/PACKAGING.md; WINDOWS_CROSS_BUILD.md |
| Installers | none yet | dc-fk66.2, .3 open |
| Linux runner `dnacalc` | recalc, diff, get, set, run; static tarball 11 MB, deb 7.8 MB; accepted on clean ubuntu:26.04 | dc-icq4.1, dc-icq4.2 closed (f7d95fa3) |
| Engine crates | not published (`publish = false`) | Cargo.toml:46; dc-qm0h open |
| Browser apps | live at dnacalc.com and onecalc.dnacalc.com, builds of 2026-11-09 | deploy/site/published.json |
| Shapes, pictures, printing, AutoFilter, sort | not yet | dc-zjsg, dc-971p.57, .37, .38 open |
| Text-to-number coercion in Enron workbooks | about 7,500 cells in 35 workbooks wrong | dc-3k5n open |
| Charts and dialog sheets | some workbooks do not open | dc-hc85 open |

Rule for every page: a claim names a figure from the latest release's notes (docs/plan/RELEASES.md
section 6), never a rate over calculated cells only. The page shows the date and the version the
figure was measured on.

## 3. What exists and is reused

| Source | What it is | Reuse |
|---|---|---|
| `cargo xtask site` (tools/xtask-harness/src/site.rs) | pack, smoke, verify, publish, rollback for the `calc` and `onecalc` sites; hashed assets; public `/build.json` | the site is a third site key in the same tool, not a second pipeline |
| `deploy/host/dnacalc-site-receive` | atomic switch on the host; `SITES = ("onecalc", "calc")` | add the new key |
| `deploy/caddy/headers.caddy` | strict CSP (no inline script or style, `connect-src 'self'`) | the content site uses it unchanged |
| `docs/fun/` (10 pages) | playground: games, Names in Motion, Watershed, Corpus Atlas, Function Bank, quiz, brochure | the playground section. Each page needs work first: inline script and style, Google Fonts and d3 from cdnjs all break the CSP; six pages have no charset (dc-jfcy) |
| `docs/fun/dna-calc-brochure.html` | one-page brochure with 2026-11-12 figures | the home page's first draft of copy |
| Handbook (handbook.dnacalc.com, own repository) | one page per function, catalogue search | linked, not rebuilt; stays on its host name |
| Web apps | DNA Calc at the apex, OneCalc on its subdomain | embedded or linked from the product pages |
| `deploy/linux/package.sh`, `cargo xtask package onecalc` | the downloads | the downloads page lists their outputs and hashes |

**One change the owner must decide:** the DNA Calc web app is served at the apex `dnacalc.com`
today. A home page there means moving the app. Proposal: the app moves to `app.dnacalc.com` (a
fourth Caddy block, same headers) and the apex serves the content site. Old links to
`dnacalc.com/` then land on the home page, whose first button opens the app.

## 4. Pages

Navigation: Home · DNA Calc · OneCalc · In the browser · Developers · Handbook · Playground ·
Blog · Download. Footer: About, Licence, Status, `/build.json`.

### 4.1 Home (`/`)
- **Message:** "DNA Calc opens your Excel workbooks and calculates them the way Excel does. On Windows
  it runs your VBA and your XLL add-ins. It is open source (MIT)."
- **Shows:** one live OneCalc box (type a formula, see Excel's answer); three cards (Windows desktop,
  browser, Linux runner); the corpus and bank figures with their date; a link to Download.
- **Before it says "runs your VBA and XLL add-ins":** a signed installer is on the downloads page
  (dc-fk66.2, .3) and the add-in and VBA figures of section 2 are re-run on that installer's binaries.
  Until then it says "a preview".

### 4.2 DNA Calc on Windows (`/desktop`)
- **Message:** "A Windows spreadsheet for Excel workbooks. Your .xlsm macros run as user functions;
  your .xll and .xlam add-ins load."
- **Shows:** screenshots of an opened workbook, an add-in's function in a cell, a short clip of
  opening a file with "Open with". A table "works / not yet" built from section 2.
- **Must be true first:** the MSI installs per user and uninstalls cleanly (dc-fk66.3); "Open with"
  works (WINDOWS_INSTALLERS.md section 5). The "not yet" list names drawing, printing, AutoFilter,
  sort and add-in events (dc-1nbp.3) until they close.

### 4.3 DNA OneCalc (`/onecalc`)
- **Message:** "One Excel formula at a time, with its inputs, its parts and a step-by-step trace.
  Load an add-in and test its functions without Excel."
- **Shows:** the web OneCalc embedded; screenshots of the desktop add-in panel; the m4 and m5 figures.
- **Must be true first:** the desktop installer exists for "test your add-in" claims; the web page
  only claims what the web build does (no XLL in the browser).

### 4.4 In the browser (`/web`)
- **Message:** "Open an .xlsx in your browser. Nothing is uploaded; the calculation runs on your
  machine in WebAssembly."
- **Shows:** a link to the app, a sample workbook to open, what does not work in the browser
  (XLL, COM, files outside the page).
- **Must be true first:** "nothing is uploaded" is checked against the CSP (`connect-src 'self'`)
  and the app's network use in the smoke run; a release newer than 2026-11-09 is live.

### 4.5 Developers (`/developers`, three sub-pages)
- **Add-ins (`/developers/addins`).** "Your XLL behaves as it does in Excel, or you get a typed
  refusal naming what is missing." Shows the XLL C API callbacks covered, the m4 sets, how to load one
  in OneCalc. Before claiming Excel-DNA or xlOil support by name: their sets in m4 pass on the release.
- **VBA (`/developers/vba`).** "CalcVBA runs VBA outside Excel: on Windows, on Linux and in the
  browser." Shows the baseline.json table per project with matched, mismatched, refused and missing.
  Before claiming "your macros": the third-party projects have Excel answers (today they are missing)
  and the `windows` section of baseline.json is filled.
- **Engine and runner (`/developers/engine`).** "Check a workbook without Excel: `dnacalc diff
  book.xlsx` compares every formula cell with the file's cached values." Shows the runner's commands
  (deploy/linux/README.txt) and exit codes. Crates: "coming" until dc-qm0h publishes; no API listing
  before then.

### 4.6 Handbook (`/handbook`, a short page linking handbook.dnacalc.com)
- **Message:** "How each Excel function really behaves, measured against Excel."
- **Shows:** the catalogue search box (links out), five example function pages, the bank figure.

### 4.7 Playground (`/play`)
- **Message:** "Spreadsheets for fun: games and models that run on Excel formulas."
- **Shows:** the docs/fun pages after the CSP rework: Circular Reference, Spill Range, Trace Error,
  Error Check, the quiz, Names in Motion, Watershed, Corpus Atlas, Function Bank.
- **Must be true first:** each page loads under the site's CSP with local fonts and a local d3, and
  its figures carry their date. The quiz says it was checked on Excel 16.0 build 20527 on 2026-11-12.

### 4.8 Blog (`/blog`)
- **Message:** what changed, how a hard function was matched, the videos' write-ups.
- **Shows:** posts from Markdown with a feed (dc-6q2k.3); each video post pairs with its script
  (dc-6q2k.4). First posts: "Why bit-match Excel", "Running VBA outside Excel", the 0.2.0 notes.

### 4.9 Download (`/download`)
- **Message:** "Preview 0.2.0. Windows installer, zip, Linux tarball and deb."
- **Shows:** per file the name, size, SHA-256, signer and the release notes link; WebView2 note;
  how to uninstall.
- **Must be true first:** files signed by the owner (WINDOWS_INSTALLERS.md section 6), notes made
  (RELEASES.md section 6), install test passed (dc-fk66.3).

### 4.10 About, Licence, Status
- **About:** who builds it, the MIT licence, the third-party notices, how to report a problem.
- **Status (`/status`):** the figures of the live release beside master's latest, each with date and
  commit (the old dc-ysl.6 "status updates"). Generated at pack time from the release notes; no live
  queries.

## 5. Look and build

- One static folder built with the existing site tool; plain HTML and CSS from the dna-strand design
  tokens (apps/dna-strand); no inline script or style; no third-party fonts or CDNs.
- Live boxes (OneCalc on the home page) load the OneCalc wasm bundle from the same origin.
- Screenshots and clips: made from release builds on the private desktop; each file named with the
  version it shows. Clips are short (under 30 s) and silent; the videos live on the blog.

## 6. Order

1. Owner agrees this plan and the app move (section 3).
2. dc-6q2k.2: the site key, home, product pages, download page (empty until signed files exist),
   status page; docs/fun rework (dc-jfcy).
3. First preview (RELEASES.md): figures and downloads filled; owner publishes.
4. Blog and videos (dc-6q2k.3, .4).

## 7. Questions for the owner

1. Move the web app to `app.dnacalc.com` and put the site at the apex?
2. Which audience leads the home page: Windows users (proposed) or developers?
3. May the home page show the corpus figure before a signed installer is out, labelled "preview"?
4. Is a name or face on the About page wanted, or the project only?
