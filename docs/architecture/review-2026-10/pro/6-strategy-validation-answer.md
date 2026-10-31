# DNA Calc: medium-term strategy and validation at corpus scale

**Assessment date: 31 October 2026**  
**Planning horizon: 12–18 months, with a concrete first 90 days**

This assessment uses the project state and architecture rulings in the supplied prompt. The reported 83% function-probe exactness and 93.6% cached-cell agreement are supplied baselines, not measurements rerun for this report. External corpus metadata, Microsoft documentation, published terms and prices were checked for this assessment. Proposed milestones, staffing allocations and capacity estimates are recommendations, not achieved results.

## Recommendation

**Make DNA Calc an embeddable Excel-compatible calculation engine whose distinguishing feature is a reproducible compatibility contract, backed by unusually useful diagnostics.** Aim first at developers and specialist model owners who need to execute, test and understand spreadsheet models outside Excel.

The product promise should be: **“Run this model, see precisely which Excel behaviour is supported, and reproduce or explain every discrepancy.”** Reference-grade compatibility is the standard to earn, rather than a claim established by the current percentages.

The first quarter should establish trustworthy evidence, resolve the Excel-oracle licensing question, and prove usefulness with three outside users. The larger spreadsheet interface, programmable extensions and large-model performance should serve that objective.

Five decisions follow:

1. Publish a named compatibility profile and its limitations; avoid an undifferentiated “Excel compatibility” percentage.
2. Make whole-workbook success, reproducibility and diagnosis first-class product outcomes.
3. Grow validation through a static corpus census, a frozen family-aware sample, modern feature probes and command transcripts.
4. Start the Excel laboratory with one or two fixed, isolated Windows environments. Scale only after measuring complete job throughput and resolving the applicable rights.
5. Use the Handbook, OneCalc and CalcVBA to strengthen one delivery path, with explicit limits on their independent scope.

**A consequential research finding:** current Microsoft Product Terms contain restrictions concerning bot-generated outputs, AI development and replication of product functionality. Buying an unattended Office entitlement does not, by itself, resolve permission to use Excel as a compatibility-development oracle. The precise application to this project needs written interpretation; section 6 explains the issue and the work that can proceed meanwhile.

## 1. Three coherent positions

| Position | First users and concrete advantage to establish | Scope and architecture priorities | First convincing public milestone | Principal risk |
| --- | --- | --- | --- | --- |
| **A. Excel-compatible calculation infrastructure with evidence** | Developers embedding existing spreadsheet models, Excel-DNA integrators, model-validation teams. Make deployment outside Excel, reproducible testing and discrepancy diagnosis unusually dependable. | Typed values and refusals; import fidelity; explicit calculation contexts; CLI/SDK; replay; function claim records; compatibility reports. A small diagnostic UI supports these. | A reproducible benchmark release, a useful CLI and one complete external integration, with every unsupported case visible. | Becoming an impressive research programme without a deployable supported subset; dependence on a lawful and reliable oracle workflow. |
| **B. A programmable spreadsheet for custom tools and agents** | Technical analysts and developers building small applications around spreadsheets. Make safe, inspectable customisation easy. | Stable command API, transactions, undo, permissions, extension boundaries, good editing UX, portable UDFs and agent-facing tooling. | Three repeatable workflows where an external user or agent builds and changes a useful spreadsheet application without owner intervention. | UI, integrations and support expand rapidly; correctness remains necessary underneath. |
| **C. A fast, explainable spreadsheet for large models** | Finance, engineering and quantitative users with genuinely slow models. Reduce waiting and make expensive dependencies visible. | Profiling, dependency explanations, memory bounds, formula blocks, incremental calculation, iteration/data tables, parallelism and responsive publication. | Several consented large models showing reproducible end-to-end and incremental improvements, with equal correctness and explicit memory measurements. | Requires representative large models and sustained performance engineering; public web corpora are weak substitutes. |

**Recommend A.** It best connects the existing engine, numerical research, architecture and Handbook to a user-visible job. Your Excel-DNA background also provides a plausible route to the first developer users. That is a distribution hypothesis to test, not proof of demand.

Open source, Rust, browser execution and function count are insufficient differentiators on their own. IronCalc already describes an embeddable Rust/WebAssembly engine, approximately 500 functions and spreadsheet import/export. Generic agent customisation is also a moving competitive target: Microsoft's October 2026 Excel update includes reusable Copilot skills and workflow recording. These are reasons to make *evidence, supported execution and diagnosis* the centre of the proposition. They do not establish which product is best on a particular workload. Sources: [IronCalc](https://www.ironcalc.com/); [Microsoft's October 2026 update](https://techcommunity.microsoft.com/blog/excelblog/whats-new-in-excel-october-2026/4549758).

### 1.1 What DNA Calc should become exceptionally good at

**Executing a declared model outside Excel.** Give an integrator one versioned engine, one reproducible execution profile and a report identifying supported features and unresolved dependencies. A useful first deliverable is a CLI plus a narrow embedding API, with a .NET adapter particularly well aligned with your existing audience. Add other bindings when a real adopter needs them.

**Explaining a difference.** The useful output is not just “cell F27 differs.” It should identify the formula and dialect, input types, relevant dependencies and settings, expected and actual typed results, numeric distance if applicable, the command that produced the observation, and a small reproducer when available. Measure the time an outside developer needs to understand a discrepancy.

**Making compatibility claims auditable.** A claim should name the Excel build/profile, DNA commit and mode, tested input region, observed exceptions and evidence. “Supported”, “observed to match”, “mathematically accurate” and “bit-identical on these cases” are different statements. The Handbook can expose these distinctions directly.

These are intended advantages to demonstrate in comparative user trials. There is no evidence yet that DNA Calc does them better than every alternative.

### 1.2 Give each existing project a bounded role

| Project | Role in the recommended strategy |
| --- | --- |
| **DNA Calc** | The production evaluator, model representation, command contract and embedding surface. |
| **DNA OneCalc** | An engine-free inspection and experimentation front end using explicit adapters. It can make formula construction, formatting and UDF behaviour convenient to inspect without creating another calculation implementation. |
| **CalcVBA** | A focused path for the UDF/procedure and typed-callback scenarios needed by the supported profile. The neutral API, ABI and lifetime work already accepted remains valuable. A complete Excel application object model is a separate commitment. |
| **Excel Functions Handbook** | A readable view of the same function facts and evidence records used by the engine and test system. Avoid a second manually maintained support matrix. |

For the next 12–18 months, deliberately exclude universal desktop-Excel UI parity, a collaboration platform, broad Power Query/Power Pivot replacement, arbitrary Excel VBA application compatibility, and a promise that every workbook can be round-tripped losslessly. Preserve the declared XLSB-to-XLSX conversion policy. Keep new numerical research bounded by evidence of user impact or a specific exactness claim.

### 1.3 Preserve the architecture sequence

The accepted command envelope and the smallest serial end-to-end slice should remain the first integration check. Corpus infrastructure can proceed in parallel, but must observe the real worker and command path as soon as that slice exists.

The next architecture investments should make evidence cheap to obtain:

- A stable trace containing command, calculation-pass and call identities, model revision and publication/freshness information.
- Typed outcomes that distinguish an Excel value or error from a DNA refusal, stale cache, unsupported dependency or failed execution.
- Explicit contexts for time, locale, calculation settings and runtime mode.
- Formula-block dialect and edit semantics that survive import, editing and saving.
- Reproducible save observations pinned to the model revision and result view specified in Appendix A.

Do not reopen the selected Exact/Fast policy. Native Exact needs CPU-specific evidence; browser Exact must expose refusals when an unmodelled operation is actually executed. Browser Fast remains separately measured. A never-executed branch containing an unsupported operation must not automatically become an execution refusal.

Thread pools, renderer choices and optional runtimes can then be justified by measured bottlenecks. Formal checks are especially useful for the small state machines governing retry, effect identity, stale callbacks and publication; broad formalisation should not delay the first usable integration.

### 1.4 Medium-term milestones and demand checks

| Period | Intended result | Evidence that justifies continuing |
| --- | --- | --- |
| Months 1–3 | Reliable conformance pipeline; documented first profile; basic diagnostic CLI; initial outside trials. | Trustworthy references, explicit failure accounting and three users who can reproduce a report. |
| Months 4–6 | A supported embedding preview for a narrow workload class; installable adapters; actionable diagnostics. | At least two independently maintained integrations, repeated use and one concrete support/sponsorship or paid-pilot signal. |
| Months 7–12 | Broader profile driven by adopter blockers; stable releases and regression evidence. | Several models used repeatedly without owner repair on every run; falling diagnostic/support burden. |
| Months 13–18 | Deepen either supported model breadth or large-model performance, according to demand. | Users can identify the improvement that changes adoption, backed by comparative measurements. |

If interviews reveal that customers primarily want debugging inside their existing Excel workflow, the diagnostic and Handbook work can support that route. If lawful oracle access cannot be established, reconsider the scope of exact-compatibility claims before committing another year to them. Neither a rising corpus score nor another hundred functions substitutes for these decisions.

## 2. Define the claim before enlarging the corpus

### 2.1 What the present numbers mean

The **83% exact agreement across 3.9 million recorded answers** describes a particular function-probe distribution. It does not reveal the percentage of functions that are reliable, how failures cluster by argument region, or how often those regions occur in real models.

The **93.6% agreement across 20,030 formula cells in 21 workbooks** describes agreement with stored values in a very small workbook collection. Refusals counting as failures is the correct policy. Nevertheless, copied formulas, workbook families, stale caches, unknown calculation settings and application provenance can dominate the result.

These percentages cannot be averaged. A workbook may also contain one numerically small discrepancy that changes a comparison and produces a materially different downstream result. Numerical proximity and functional equivalence therefore need separate reporting.

The first diagnostic task is to map current failures into independent causes: import, missing capability, dependency/command semantics, numerical implementation, reference defect or environment. That map should determine the next work allocation.

Give symbolic/numerical search its own independent evaluation split. Cases used to select a program or tune an algorithm are search evidence, even if they originally came from Excel. Reserve unseen input-generation families and argument regions, including independent boundary and adversarial cases, and report their results separately. A family-separated workbook holdout does not prevent fitting the 3.9 million function answers. Keep the numerical holdout sealed from search workers and version any later promotion into the regression set.

### 2.2 Publish a concrete initial profile

Proposed starting profile:

> Self-contained formula models in explicitly listed input formats, evaluated under a named Windows desktop Excel build, with recorded workbook settings, no unprovided external services or add-ins, and a stated DNA platform/mode. Unsupported required capabilities remain failures within the declared cohort.

Start the production claim with a tractable XLSX-oriented profile. Maintain an all-format diagnostic population alongside it. A legacy XLS file converted by Excel into XLSX can help isolate evaluator correctness, but it does not prove native XLS ingestion. Similarly, XLSB-to-XLSX conversion is a named conversion path, not same-format preservation.

A profile identifier should resolve to:

- Excel product, exact build, update channel and 32/64-bit architecture.
- Windows build, CPU vendor/model/stepping and exposed instruction capabilities.
- UI language, regional settings, separators, time zone and workbook date system.
- Calculation mode, iteration parameters, precision-as-displayed setting, calculation-thread setting and add-ins.
- External-dependency policy, fixture construction method and command sequence.
- DNA version, target platform, Exact/Fast mode and relevant configuration.

A channel label such as “Current” is not a reproducible version. A result that matches one Excel configuration must not automatically count as a match for another.

## 3. Public corpora: availability, rights and appropriate use

The most useful programme combines old business models, heterogeneous public files, modern examples, owned semantic fixtures and a small contemporary partner collection. No public corpus below represents ordinary 2026 Excel usage by itself.

### 3.1 Corpus inventory

| Collection | Scale and licence of the cited release | Recommended use and important limitations |
| --- | --- | --- |
| **Enron** | The original extraction reports 51,572 Excel attachments and **16,189 MD5-distinct spreadsheets**; use approximately 16,000 until the chosen archive is inventoried. The Figshare release declares **CC BY 4.0**. | Real business models, names, links and related versions. Old files from one company; modern arrays and LAMBDA are absent. Avoid publishing associated personal email material. [Original author](https://www.felienne.com/archives/3634); [dataset](https://figshare.com/articles/dataset/Enron_Spreadsheets_and_Emails/1221767); [licence metadata](https://api.figshare.com/v2/articles/1221767). |
| **FUSE** | The paper reports **249,376 unique spreadsheet binaries**, separately from 2,127,284 URL records. The cited Zenodo release declares **CC BY 4.0**. | Broad ingestion and preservation stress. The paper reports 205,978 zero-formula files and 28,616 files unreadable by its tooling. Predominantly legacy XLS, not an XLSX-ready population. Its original 1 MB binary capture ceiling makes it weak evidence for large-model performance. Counts are historical analysis results, not a fresh census. [Paper](https://static.barik.net/barik/publications/msr2015/PID3640389.pdf); [archive](https://zenodo.org/records/581678); [licence metadata](https://zenodo.org/api/records/581678). |
| **EUSES** | Commonly described in the literature as **4,498 unique workbooks**. The current mirror states **5,607 total / 4,499 unique processable files** and declares **CC BY 4.0**. | Cheap, varied smoke tests, mainly older spreadsheets. Record the release and actual manifest rather than silently reconciling the one-file discrepancy. [Archive and description](https://zenodo.org/records/581673); [licence metadata](https://zenodo.org/api/records/581673). |
| **VEnron / VEnron2** | Original VEnron: **7,294 files in 360 evolution groups**. VEnron2: **12,254 in 1,609 groups**. A separate licence for the added grouping annotations was not verified. | Excellent family information and version-change material. These are derived from Enron, not independent coverage. Version pairs are not exact command logs. Check annotation rights and availability before making them a dependency. [Authors' SpreadCluster paper](https://arxiv.org/pdf/1704.08476). |
| **SpreadsheetBench** | **912 forum-derived tasks**, with multiple workbook cases per task; the project declares **CC BY-SA 4.0**. | Useful modern, difficult examples. It is a spreadsheet-manipulation benchmark, not an Excel-engine oracle. Split by original task/thread and independently capture the required calculation observations. [Official repository](https://github.com/RUCKBReasoning/SpreadsheetBench). |
| **Sheetpedia** | The published table describes approximately **325,000 source workbooks yielding 290,000 unique worksheets**. It includes Enron, FUSE and ExcelForum material; the project declares **CC BY-SA 4.0**. | A promising source of more modern material. Its units and preprocessing matter: these are not 300,000 independent intact workbooks. Audit transformations and remove cross-corpus overlap before using it for recalculation. [Official repository](https://github.com/TTtianTT/Sheetpedia). |
| **GitHub-mined files** | No single corpus or universal licence. Retain repository, commit, path, hash and the applicable file/data licence. | Modern fixtures and real developer regressions. Public availability grants no general reuse licence; forks and copied third-party fixtures create duplication and rights ambiguity. [GitHub's licensing guidance](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/licensing-a-repository). |
| **Microsoft samples** | Terms vary by download and repository. For example, OfficeDev's Office-Add-in-samples repository has an **MIT licence**; that does not establish the licence of all Microsoft templates. | Good feature examples and small reproduction seeds. Many samples contain data rather than substantial calculation. Inspect each asset's provenance and terms. [Example repository licence](https://github.com/OfficeDev/Office-Add-in-samples/blob/main/LICENSE). |

The exact Enron, EUSES and FUSE licence declarations above were verified through their first-party JSON metadata, not inferred from another benchmark's README. Those declarations should still be retained with each acquisition. A depositor's licence declaration does not establish that every embedded third-party item or personal detail is freely redistributable.

CC BY requires attribution and change notices; CC BY-SA also has obligations for redistributed adaptations. Keep fixture licensing separate from engine-code licensing and review what is actually being redistributed. Creative Commons itself notes that privacy and other rights can remain relevant. Sources: [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/); [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).

There is already a closely related **Apache-2.0 corpus harness**, [witanlabs/xlsx-corpus-bench](https://github.com/witanlabs/xlsx-corpus-bench). Its acquisition and adapter machinery merit inspection. Its documented numerical comparator uses 1e-9 relative tolerance, its volatile exclusion examines the cell's own formula, and it acknowledges imperfect reference caches. Audit any reused machinery; its percentages are not DNA Calc bit-exact evidence.

### 3.2 Build a corpus registry before an Excel farm

Preserve the original bytes in content-addressed storage. Record the acquisition URL/date, source release, declared rights, SHA-256, original format, any conversion, family identifier and analysis-tool version. Keep extracted metadata and run results separate from source files.

Perform a cheap static census first: actual file type, workbook and sheet structure, formula count, formula templates, names, tables, array/spill metadata, external references, macros, calculation settings and size. Formula-free files belong in the codec suite and should not inflate calculation success.

Use two levels of deduplication:

1. Exact file hashes and normalised package/content fingerprints.
2. Related-model clustering using formula structure, sheet structure, source origin, template similarity and known version groups.

Retain variants because they exercise different inputs and edits, but do not count each monthly copy as an independent model. Conservative clustering mistakes should be visible and revisable. Put related files, repository forks, forum-task variants and known evolution groups into the same evaluation partition.

### 3.3 Keep distinct sampling purposes visible

| Suite | Selection | What its score can establish |
| --- | --- | --- |
| **Population sample** | Stratified random sample from a declared acquisition population, selected before running DNA. | Performance on that population and profile, with known sampling weights. |
| **Feature challenge suite** | Deliberate oversampling of rare, modern and difficult features. | Coverage of named behaviours; not a population prevalence estimate. |
| **Regression and command suite** | Minimized failures and owned temporal/edit fixtures. | Continued correctness of known obligations. |
| **Partner models** | Contemporary models supplied with explicit permission and a stated operational goal. | Usefulness for actual adopters and realistic performance, with the selection bias acknowledged. |

Stratify by source/era, file format, formula-count bands, distinct formula shapes, function families, cross-sheet dependencies, names/tables, arrays, dynamic references, cycles, data tables, macros/UDFs, external dependencies and locale-sensitive content. Include a genuinely large-model tier obtained from consenting users or clearly labelled synthetic generators.

For the first release, reserve 500 family-distinct cases for sealed evaluation if acquisition permits. At a total of 2,000 families, a coherent allocation is 1,200 development, 300 rolling validation and 500 sealed evaluation. As the population grows, move toward a 70/15/15 split while retaining an adequate sealed sample. The fixer must not inspect sealed cases. Once a sealed failure is studied, it is known regression material; later unseen claims need a replenished holdout. Keep versioned historical scores so replenishment cannot rewrite past results.

Use family-level resampling or stratified uncertainty estimates. Millions of copied cells do not provide millions of independent observations. Any statistical interval describes the declared sampled population, not all spreadsheets in use.

## 4. Comparison protocol and honest headlines

### 4.1 Maintain two denominators

Publish both **conformance among reference-ready cases** and **verified coverage of the entire registered sample**.

For example, suppose 10,000 sampled workbook families contain 8,000 reproducible references, and DNA fully passes 6,000:

| Outcome | Count |
| --- | ---: |
| Verified workbook passes | 6,000 |
| Known failures against a usable reference | 2,000 |
| Reference unresolved, unavailable or unstable | 2,000 |
| Entire frozen sample | 10,000 |

The report is **75% conformance on reference-ready workbooks and 60% verified whole-sample coverage**. The unresolved cases are not verified successes. They are also not automatically proven DNA defects. Explain their causes separately.

Within every reference-ready cohort, refusals, unsupported required features, import failures, crashes and timeouts count as failures. A restricted profile is legitimate when declared before measurement; it must not quietly replace the broader all-sample headline.

For cell-level denominators, derive expected formula locations and result regions independently of DNA's importer. If DNA cannot import a sheet whose formula inventory is known, all of its expected observations remain failures. If even the independent inventory cannot determine a cell count, report the unknown count and workbook outcome; do not manufacture a cell denominator.

### 4.2 Publish a small scorecard

The main report should contain:

- Acquisition counts: raw files, exact-distinct files, families, formula-bearing files, selected files, reference-ready and unresolved files.
- Whole-workbook pass rate for calculation, and a stricter workflow pass rate including the declared preservation/edit obligations.
- Formula-result agreement, output-cell agreement and mean workbook agreement, plus formula-template and feature breakdowns.
- Numeric exactness, ULP-distance distribution and a separately declared practical-accuracy measure.
- Refusals, missing features, crashes, timeouts and resource use by reason.
- Round-trip and command-transcript scores.
- Excel-to-Excel drift, repeatability and oracle failure rates.

Publish separate results for native Exact, native Fast, browser Fast and browser Exact; distinguish native operating systems and CPU profiles where relevant. Do not merge a refused Exact computation into a successful Fast score. A generic epsilon must never convert a Fast result into an Exact claim.

For practical numerical accuracy, predeclare function- or workload-appropriate absolute and relative budgets, for example **|a-b| ≤ atol + rtol × max(|a|,|b|)**. Report the budgets, near-zero behaviour and actual error distribution. Separately compare downstream types, errors, branch choices and spill shapes exactly; a small numeric difference can change them. Never select a tolerance after inspecting a failure merely to turn it into a pass.

### 4.3 Preserve four observations

Retain **the source file, its original caches, freshly observed Excel results, and DNA results**. An opened-and-saved Excel file is a new artifact, not an unmodified source.

Original caches are useful for diagnosing changes and measuring agreement with historical artifacts. Their generating application, freshness and settings may be unknown. A decimal parsed from XML is not the same observation as a live Excel binary64 value.

For live numeric capture, use bulk `Range.Value2` reads and record each double's raw bits before decimal formatting or JSON conversion. Preserve error tags, booleans, strings and empty values separately. Blank, zero and the empty string are distinct cases. Microsoft documents that `Value2` avoids the Date/Currency conversions performed by `Value`. [Value2 documentation](https://learn.microsoft.com/en-us/office/vba/api/Excel.Range.Value2).

Also record fixture construction: a numeric input entered through Excel's UI, a formula literal, a `Value2` assignment and a number loaded from a file need not exercise the same input-conversion path.

### 4.4 Reference execution

1. Inventory the immutable original without executing it. Extract its stored settings and caches.
2. Open a copy in a prepared, isolated Excel session under the declared macro, add-in, link and connection policy. A temporal fixture must define startup and opening behaviour, configure calculation before `Workbooks.Open` using a controlled bootstrap if necessary, and verify that opening has not consumed the intended cached initial state.
3. Record actual settings and any repairs or representation changes. Do not silently treat a repaired file as the original.
4. For the full-recalculation suite, run the prescribed full rebuild and capture the resulting typed observations. For command tests, execute precisely their specified actions instead.
5. Validate reference repeatability from a clean initial state. Start with all pilot cases, then retain comprehensive checks for sensitive strata and an audited repeat sample for the bulk population.
6. Run DNA from the equivalent initial state and context. Compare structure/types before values and numeric bits.
7. Save only to a new artifact and execute the separate preservation tests.

`CalculateFullRebuild` rebuilds dependencies and recalculates all open workbooks; it is not equivalent to ordinary recalculation or range calculation. Excel also shares calculation mode across open workbooks, with the first opened workbook influencing the session. These are concrete reasons for controlled sessions and explicit commands. Sources: [full rebuild](https://learn.microsoft.com/en-us/office/vba/api/excel.application.calculatefullrebuild); [calculation-mode inheritance](https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/current-mode-of-calculation).

Do not equate one observation of `CalculationState` with a universal notion of “settled.” The pure synchronous suite can use a narrow completion condition. Async UDFs, connections, RTD and effectful VBA need protocols that identify which command and result generation are being awaited. Microsoft's recalculation description discusses additional passes after asynchronous results. [Excel recalculation](https://learn.microsoft.com/en-us/office/client-developer/excel/excel-recalculation).

### 4.5 Special behaviours need explicit experiments

**Volatility and external state.** RAND-family outputs cannot simply be expected to repeat across independent sessions. NOW/TODAY depend on time; external data and UDFs can add further state. Keep original cases visible as nondeterministic or unresolved when a valid exact reference cannot be obtained. Test relevant invariants and behaviour separately. A fixture rewritten to inject a clock or fixed random input is a derived controlled experiment, not a pass for the untouched original. Track affected dependents, not just formulas containing a volatile function name.

**Errors and missing dependencies.** A correct `#DIV/0!` is a successful value comparison. Matching `#NAME?` because both environments lack a workbook's required UDF does not establish successful execution of that workbook. Preserve an explicit unresolved-dependency classification. Likewise, a DNA refusal rendered like an Excel error must remain a refusal in machine-readable results.

**Arrays and spills.** Check formula anchors, result shape, the complete output region and unexpected extra cells. Agreement over the overlapping portion of two differently sized spills is a failure. Include obstructions, spill resizing and dependent references. Keep the formula-anchor metric distinct from output-cell weighting.

**Formula dialect.** Preserve `Formula`/`Formula2` through construction, import, edits and saving. Microsoft describes their different implicit-intersection and array-evaluation semantics; setting a modern fixture through the wrong API can change the experiment. [Formula versus Formula2](https://learn.microsoft.com/en-us/office/vba/excel/concepts/cells-and-ranges/range-formula-vs-formula2).

**Iteration and data tables.** Record the initial cached state, iteration threshold/count, calculation mode and command sequence. Recalculating repeatedly until an appealing answer appears is not a reference protocol. Exercise data-table scenario isolation and the accepted iteration-session boundary explicitly.

### 4.6 Formatting and file preservation

Measure three formatting layers independently:

1. **Stored semantics:** number-format codes, styles, conditional-format rules/ranges, row and column sizes, hidden state and related metadata.
2. **Displayed semantics:** formatted text and effective conditional formatting for selected cells, under fixed locale and layout settings.
3. **Rendering:** a small curated visual suite with known fonts, DPI, zoom and viewport, using suitable image-difference tolerances and review.

`Range.Text` is width-sensitive and is unsuitable as a numeric oracle. `DisplayFormat` can expose conditional-format effects that ordinary style properties do not reflect. Sources: [Text](https://learn.microsoft.com/en-us/office/vba/api/excel.range.text); [DisplayFormat](https://learn.microsoft.com/en-us/office/vba/api/excel.range.displayformat).

Run both no-edit and prescribed-edit round trips: original → DNA import → DNA save → Excel reopen → structural checks and recalculation. Include an Excel-save/Excel-reopen control to understand normalisation. Compare meaningful formulas, dialects, names and scope, tables, validation, conditional formatting, calculation settings and required package relationships. ZIP byte equality and successful reopening are each insufficient.

Recalculation after reopening can conceal bad saved caches. Inspect saved cache/freshness claims before recalculation as well. Required opaque features need explicit preservation or an explicit refusal/loss report. Unsupported content silently dropped on save fails the relevant preservation claim.

XLSB-to-XLSX must remain labelled conversion. Macro-bearing formats also need an explicit preservation contract; conversion to XLSX cannot be described as preserving executable macro content.

### 4.7 The architecture needs command transcripts

Static corpus recalculation will not validate the most consequential changes in Appendix A. Build owned, replayable transcripts covering:

- Manual range calculation and command-scoped freshness.
- Name shadowing, rebinding and explicit name evaluation.
- Copy, move, insert and sheet operations on formula blocks.
- A delayed async result after a cell has moved or been overwritten.
- A resumed typed callback or retry that must not repeat an effect.
- The spill-blank race and resizing/obstruction transitions.
- Cached VBA reads under the accepted conservative fences.
- Iteration initialisation and data-table restoration of the base model.
- Save during calculation, pinning the specified revision and result view.
- Cancellation, restart and recovery obligations that the contract actually promises.

Capture intermediate observations and effect records, not just the final worksheet. Use deterministic scheduling in synthetic races. For DNA-specific guarantees without an Excel equivalent, verify the stated invariant directly and label it accordingly.

## 5. Turn failures into evidence and fixes

Use a single lifecycle: reproduce → classify → cluster → minimise → decide → fix → retain regression → rerun affected populations.

### 5.1 Classification and priorities

Separate reference/harness defects, file import, unsupported required features, evaluation/dependency errors, numerical differences, formatting/preservation, and resource failures. A displayed error code alone is not a root-cause signature.

Prioritise approximately by **distinct target workbook families affected × user importance × diagnostic confidence**, balanced against effort and architectural risk. A repeated formula block containing 100,000 failures should not automatically outrank a small blocker that prevents five adopters from using their models.

Track primary causes and downstream consequences. Fixing one parser error may remove thousands of numeric-looking discrepancies; counting those as independent algorithm discoveries misallocates work.

### 5.2 Shrink the failing experiment

A reducer should output a small workbook or fixture, the initial state, the command transcript, required environment/dependencies, and typed expected/actual observations. Its predicate must retain the relevant failure mechanism, not merely any mismatch.

Begin with dependency slicing, but preserve dynamic reads, blank observations, names, table context, hidden/filtered state and workbook settings. INDIRECT, context-sensitive functions and VBA reads can invalidate purely static slices. Then reduce sheets, ranges, formulas, inputs and action sequences, rechecking both sides after each accepted change.

For temporal failures, a clean full rebuild may erase the bug; retain the relevant sequence. For numerical failures, record exact input bits and avoid changing evaluation order accidentally. Floating-point addition does not justify unrestricted reassociation as a metamorphic property.

Publish minimized original cases only where their rights permit it. Otherwise publish an independently constructed reproduction with appropriate attribution and provenance, or an aggregate description. Minimized content can still carry third-party or private material.

### 5.3 Keep reference integrity independent of the fixer

The worker proposing an engine fix must not silently replace expected results, adjust a tolerance or remove a sample. Version comparator and reference changes, retain the prior observations and require an explicit reason for each change.

Audit the harness with deliberate faults: a dropped formula, a single-bit numerical change, a stale-cache fallback, an extra spill cell and a refused operation displayed as an ordinary error. Verify that each is rejected and counted. This is a high-value test of the evidence system itself.

Record Excel-to-Excel differences as their own findings. Do not choose whichever Excel build makes DNA look correct, and do not use a vote among other spreadsheet engines as Excel truth. Other engines can be useful for diagnosis, with their own semantics and licences identified.

## 6. A manageable Windows/Office laboratory

### 6.1 Resolve four different rights/support questions

The laboratory needs answers to four distinct questions: Windows deployment rights, unattended Office use rights, permission for the intended use of generated outputs, and engineering support/reliability. An activated copy of Excel does not establish all four.

**Unattended entitlement.** Microsoft's documented route requires the account performing the unattended task to have Microsoft 365 Apps for enterprise (unattended). Its overview describes one unique physical or virtual operating-system environment and explicitly says RPA does not support shared computer activation. A human-user subscription's installation allowance is not a bot-fleet allowance. [Microsoft's unattended overview](https://learn.microsoft.com/en-us/microsoft-365-apps/licensing-activation/overview-unattended).

The Microsoft 365 A3/E3 unattended terms identify dedicated hardware, Azure including AVD, and Windows 365 Enterprise as deployment options, subject to their conditions. They also refer to one unique Apps instance and normal reassignment rules. Obtain confirmation of what this means for parallel processes, fixed VMs, rebuilds and archived images; do not infer a precise EXCEL.EXE process-count rule from the word “instance.” [Microsoft 365 Product Terms](https://www.microsoft.com/licensing/terms/en-US/product/Microsoft365/All).

**Output use is a separate, material question.** The MCA Microsoft 365 Applications terms contain a section headed “Use for Training and AI Models.” The output-use restriction includes “replicate product functionality.” Similar text appears in current Office desktop/LTSC terms, and the universal software restrictions also mention replication. The heading, wording, applicable agreement and legal interoperability provisions require interpretation for this particular workflow. Sources: [Microsoft 365 Applications](https://www.microsoft.com/licensing/terms/productoffering/Microsoft365Applications/mca); [Office Desktop Applications](https://www.microsoft.com/licensing/terms/en-US/productoffering/OfficeDesktopApplicationsWindows/MCA); [universal software terms](https://www.microsoft.com/licensing/terms/product/ForallSoftware/all).

I cannot establish from these public pages which agreement governs your existing copies or whether the intended deterministic testing, symbolic search and AI-assisted implementation are permitted. It would be equally unjustified to declare every compatibility test prohibited or to assume all these uses are covered by purchasing an unattended SKU.

Get a written response from Microsoft and qualified licensing counsel that describes the actual work: bulk COM-driven probing, retention of exact results, implementation of a separate engine, symbolic search, showing observations to AI coding assistants, and publication of reports, Handbook entries and fixtures. Distinguish AI-assisted inference from model training/fine-tuning explicitly. Ask separately about any future service exposing Excel to third parties; internal R&D permission would not automatically cover that.

Audit the provenance of the existing 3.9 million answers: collection dates, products, agreements, method and intended use. This is not a conclusion that past collection was impermissible. Preserve the evidence and avoid expanding an uncertain use until the applicable position is established. Changing to a logged-in session, UI automation or LTSC is not an established workaround.

**Engineering support.** Microsoft permits an unattended licensing route but describes operational use as “AS IS”; its guidance does not support unattended non-interactive server-style Office automation. It identifies STA/global-resource problems and recommends VM isolation for multiple instances. [Unattended engineering considerations](https://learn.microsoft.com/en-us/office/client-developer/integration/considerations-unattended-automation-office-microsoft-365-for-unattended-rpa).

### 6.2 Recommended starting deployment

Use the existing Windows machine for interactive investigation and an initial controlled oracle lane once rights are confirmed. Add **one fixed Windows 11 environment** when either separation from development or measured throughput justifies it. Use independent VMs for concurrent Excel lanes. The orchestrator can remain on Linux.

For ease of management, Windows 365 Enterprise is attractive if the necessary tenant/licensing infrastructure already exists. An Azure VM offers more control over images, running hours and CPU-vendor families. Owned hardware offers stable physical CPU identity and predictable long-running cost. A full AVD deployment is most attractive when it already exists in the organisation.

| Option | Best role | Rough cost and qualification |
| --- | --- | --- |
| Existing dedicated Windows hardware | Initial baseline and interactive fault investigation. | No new hardware purchase; applicable Windows/Office entitlements and operating costs still matter. |
| Additional physical desktop | Second CPU vendor or sustained execution. | **Planning allowance:** US$700–1,500 for suitable 32 GB/SSD hardware, plus applicable software and local costs. This is not a current vendor quotation. |
| Azure Windows VM | Scripted fixed lanes; controlled schedules; CPU-family choice. | **Budget assumption:** $0.15–0.35 per running hour gives $24–56 for 160 hours or about $110–256 for 730 hours, before disks, networking and applicable licences. Obtain an actual regional SKU quote before purchase. |
| Windows 365 Enterprise | Persistent managed remote workstation. | Microsoft's South African page lists **US$41/month for 2 vCPU/8 GB/128 GB**, or **US$66/month for 4 vCPU/16 GB/128 GB**, excluding VAT. These are Cloud PC prices, not all-in Excel bot prices. |
| Azure Virtual Desktop | Existing organisational desktop infrastructure or later managed access. | Azure infrastructure and applicable access/product rights are separate. Multi-session density is not a reason to share one Excel environment among independent oracle jobs. |

Published sources: [Windows 365 South African pricing](https://www.microsoft.com/en-za/windows-365/enterprise/compare-plans-pricing/); [Windows 365 requirements](https://learn.microsoft.com/en-us/windows-365/enterprise/requirements); [AVD licensing](https://learn.microsoft.com/en-us/azure/virtual-desktop/licensing).

Two 4-vCPU Cloud PCs therefore have a published base price of **$132/month excluding VAT**, plus applicable prerequisites and unattended Office rights. Windows 365 Enterprise ordinarily requires Windows E3, Intune and Entra ID P1 entitlements as well as the Cloud PC licence; the chosen bundle must be checked. I did not establish a reliable current public price for the relevant unattended SKU. Request a written per-lane quote rather than budgeting with an ordinary human Office subscription price.

Do not assume one entitlement can float across an arbitrarily autoscaled clone fleet. Fixed licensed lanes that can be reset are simpler to reason about. Confirm snapshot, rebuild and reassignment treatment, and distinguish deallocation from merely shutting down a cloud guest when estimating savings.

### 6.3 Automation architecture

Use a small **C# worker with an STA thread and message pump**, fitting your existing expertise. Serialize COM object-model calls within that worker and keep COM references there. Parallelism comes from independent workers and Excel environments; extra COM calling threads are not extra calculation engines.

Start with one active workbook job per Excel process and one process per VM lane. Use a fresh process for untrusted workbook jobs initially. Reuse is a later optimisation requiring evidence that settings, add-ins, links, names and calculation state have been reset reliably.

Run Excel in a prepared logged-in user session, while a separate supervisor owns the queue and deadlines. An interactive session helps operationally but does not change an unattended workload into an attended one. Configure unattended activation using Microsoft's documented mechanism for the licensed account.

Record separate launch, open, calculation, capture, save and close stages, and both controller and Excel process identities. The supervisor should detect hangs without depending on a responsive COM call, retain diagnostics, terminate only that job's owned processes, and verify that no owned Excel instance survives before retrying. Retry at most once from a clean state and retain both attempts. Escalate to lane reset if a process restart fails, then run known-good sentinels before accepting new jobs.

Read values in bounded bulk ranges, not one COM call per cell or the entire million-row grid. Measure capture and save time as well as calculation. Unexpected dialogs are classified evidence; a general-purpose “click Yes” agent should not decide how a reference workbook is repaired or altered.

Public workbook processing needs isolation from the owner's normal desktop and credentials. Disable untrusted VBA and external refresh; enforce an XLM policy too. Microsoft's `AutomationSecurity` documentation explicitly says its force-disable setting does not disable Excel 4.0 macros. Use a separate trusted-fixture lane for necessary macro/UDF experiments. [AutomationSecurity](https://learn.microsoft.com/en-us/office/vba/api/excel.application.automationsecurity).

Blocking links or macros does not prove that a workbook needing them works. Preflight and outcome records must make that dependency visible. Permit only the required activation/update connectivity outside untrusted execution and avoid exposing the worker to production shares or unrelated secrets.

### 6.4 Build, locale and CPU coverage

Avoid the full Cartesian product of every workbook, build, language, CPU and thread count. Use a small informative matrix:

| Profile | Workload |
| --- | --- |
| Primary supported build, pinned for each benchmark release | All selected primary-profile cases. |
| Current-channel canary | Representative subset, new features and historical drift cases. |
| Preview/Insider profile | Small discovery suite; clearly separate from production compatibility. |
| Second CPU vendor | Numerical probes and workbooks whose discrepancies justify it. |
| Locale variants | Parsing, collation, date interpretation, formatted output and formula-localisation probes. |
| Historical build | Reproduction of a specific change or supported customer's environment. |

Use Office Deployment Tool configuration for the exact channel, version, bitness and language packages. Keep manifests and permitted installation media, and maintain a patched operational lane alongside historical reproductions. Availability and support of old builds are not unlimited. Sources: [ODT options](https://learn.microsoft.com/en-us/microsoft-365-apps/deploy/office-deployment-tool-configuration-options); [channel overview](https://learn.microsoft.com/en-us/microsoft-365-apps/updates/overview-update-channels).

Start with en-US plus a decimal-comma locale such as de-DE and a deliberately different UI/region combination. Add language-specific probes according to observed defects. UI language, Windows regional settings and invariant stored formulas are separate dimensions. Add 32-bit Excel only where a concrete ABI, legacy add-in or compatibility commitment requires it.

For controlled vendor comparisons, Azure Dsv5 is Intel-based and Dasv5 is AMD-based, but both document multiple possible CPU generations. Record actual CPUID details after reprovisioning or migration. Do not assume every numerical result differs by vendor; let probes establish where it matters. Sources: [Dsv5](https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/general-purpose/dsv5-series); [Dasv5](https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/general-purpose/dasv5-series).

### 6.5 Measure complete jobs before buying capacity

Use:

**jobs/hour = 3,600 × independent lanes × useful utilisation ÷ mean complete-job seconds.**

Complete-job time includes launch, open, calculate, capture, save, cleanup and the effect of retries/recovery. Report p50/p95/p99 by stratum, completed cases per allocated vCPU-hour, peak memory and the slow-tail population. State whether retry time is already included in the measured mean so it is not counted twice.

For an illustrative, unmeasured budget, two lanes at 70% useful utilisation and 30 seconds per complete run yield **168 runs/hour**. Ten thousand primary runs take about **60 elapsed hours**; two complete reference captures take about **120 hours**, before alternate configurations and round trips. At 120 seconds per run, those estimates become four times larger. This is why a 200-workbook pilot is more useful than a promised “instances per core” number.

Benchmark 2 versus 4 vCPUs and selected calculation-thread settings on the same cases. Excel's multithreaded calculation and its COM control path have different constraints; some expensive workbook features can dominate serial work. Expand to four lanes only if the queue and owner-time savings justify the additional licensing and administration.

### 6.6 Benchmark the engine separately

Oracle throughput is an operating-cost measurement. Product performance needs a separate protocol on fixed, correctness-qualified workloads:

- Cold import plus full calculation, with process-start costs clearly included or separately reported.
- Warm full recalculation under a fixed context and thread configuration.
- Incremental calculation after a prescribed input edit or structural command.
- Time to the first **fresh** visible result and time to the requested command's completion.
- Peak memory and total work for a realistic scenario batch.

Compare the same inputs, settings and declared correctness level. Keep incomplete, refused and timed-out workloads in the population's completion statistics even when latency summaries use only completed jobs. A cached display is not a fresh result, and one fast kernel is not evidence of a faster modelling workflow.

## 7. Turn tips, tricks and release changes into a continuous test source

Treat the separate quirks catalogue as a **discovery queue linked to executable evidence**. A useful entry is:

> source URL and date → behavioural hypothesis → minimal workbook/command sequence → tested Excel profiles → observed result → confidence/limitations → regression IDs → Handbook entry.

Use three feeds:

- **Official changes:** Microsoft Support's function catalogue, Microsoft 365 Apps build histories, the Excel blog and Insider announcements.
- **Primary practitioner observations:** authors' reproducible experiments, Excel-DNA issue reports and original forum examples. FastExcel's short-circuiting investigation is an example of a lead worth reproducing, not a substitute for current profile-specific evidence.
- **Real failures:** corpus clusters and partner reports, which often expose interactions a function catalogue misses.

Useful sources: [Microsoft's function catalogue](https://support.microsoft.com/en-us/excel/excel-functions-alphabetical); [build history](https://learn.microsoft.com/en-us/officeupdates/update-history-microsoft365-apps-by-date); [FastExcel's short-circuiting experiment](https://fastexcel.wordpress.com/2023/01/03/short-circuiting-excel-formulas-if-choose-ifs-and-switch/).

The recent changes are a reason to monitor value-model behaviour as well as function names. Microsoft's September 2026 announcement describes Insider work on lists, arrays in cells and nested arrays. That merits an explicitly experimental probe and a review of representation boundaries; it does not justify expanding the initial production profile before the behaviour is available and measured. [September 2026 announcement](https://techcommunity.microsoft.com/blog/excelblog/whats-new-in-excel-september-2026/4549755).

Recommended cadence: a weekly automated source diff and deduplicated intake; a short weekly human triage; a monthly batch of verified additions tied to tested builds. For each important finding, add boundaries, coercion/error cases and a few interactions with names, tables, arrays or editing. Test whether a feature is actually enabled; an announcement date or build string alone may not identify rollout state.

A manageable initial target is **5–10 well-supported behavioural additions per week**, with provenance and regression tests. Do not turn every article into a feature request. Keep “documented”, “observed on these builds”, “inferred” and “unconfirmed report” as separate evidence states.

## 8. The first 90 days

The targets below concern evidence and execution. They are not a prediction of the final compatibility percentage. The live-Excel portions depend on establishing the necessary rights; static analysis and independently authorised work can proceed concurrently.

### Days 1–14: make the first measurement trustworthy

**Owner:** select the initial target users and profile; obtain advice and written clarification on the actual oracle workflow; conduct the first two or three adopter conversations.

**Engineering:** freeze the result schema, comparator rules and original 21-workbook manifest. Land or connect to the small architecture slice. Establish the acquisition registry and a 200-workbook pilot spanning several feature/size strata.

**Outcomes:** every original workbook has an explained cache-versus-live comparison where a lawful reference is available; numerical results retain raw bits; all pilot cases have a terminal status; initial timing distributions and failure classes are visible; the first command transcripts exercise the accepted invariants.

### Days 15–30: automate without hiding failures

Inventory at least 10,000 candidate files and expand to full archive metadata where practical. Establish exact deduplication and initial family grouping. Freeze development/validation/holdout partitions before fixes use the new cases.

Target **1,000 selected workbook attempts**, including at least **500 family-distinct formula cases with usable references**, subject to the acquisition census. Record original-format and converted-format results separately. Prove a long batch can classify hangs and recover without manual deletion of problem files.

Publish an internal dashboard with both denominators, the main modes and the ten largest independent defect clusters. Complete approximately 20 modern-feature fixtures and 10 command transcripts. At day 30, decide whether the oracle rights, reference reliability and initial user interest justify scaling.

### Days 31–60: increase breadth and close important gaps

Target **5,000 selected source-workbook attempts** and at least **1,000 independent formula-model families**. Begin a 300–500-case alternate-build/CPU/locale subset, chosen for information value rather than equal matrix coverage.

Develop approximately 100 owned modern-feature probes, 30 command transcripts and 50 preservation fixtures. Minimise and classify the leading failures; aim for 25 durable regressions and fixes to the highest-value causes. Every accepted fix receives a regression and an affected-population rerun.

Put the CLI and report in front of the first design partners. Measure how long it takes them to identify a limitation or reproduce a discrepancy. Profile ingestion and result capture before assuming calculation is the bottleneck.

### Days 61–90: publish a defensible baseline and make the next decision

Target **10,000 preselected source workbooks with terminal outcomes**, with unique-file, family, formula-bearing, reference-ready and conversion counts all published. A target of **2,000 or more distinct formula-model families** is useful if the census supports it; disclose the achieved population rather than filling the target with near-duplicates. Keep at least **500 family-distinct cases** sealed for the first formal evaluation if acquisition permits.

Complete approximately **200 modern-feature probes, 50 command transcripts, 100 preservation fixtures and 50 minimized regressions**. These sets may overlap; do not add their counts to claim independent workbook coverage.

Publish a versioned compatibility report, acquisition/reference/comparator manifests and legally publishable reproducers. Present performance on correctness-qualified cases as well as completion/resource statistics for the entire sample. Include one end-to-end walkthrough showing import, execution, discrepancy diagnosis and safe saving.

Have **three external users** attempt a real supported workload. At least two should be able to repeat the process without a special owner patch. Their recurring blockers determine the following quarter's support profile.

### Staffing and review rhythm

| Resource | Main assignment |
| --- | --- |
| Linux worker A | Corpus inventory, independent extraction, deduplication and reduction tools. |
| Linux worker B | Engine integration, command/regression suites and selected semantic fixes. |
| Windows worker | Excel references, build/locale/CPU probes and oracle diagnostics. |
| Orchestrator | Queues, manifests, result collation, cluster prioritisation and evidence consistency. |
| Owner | Scope, licensing interpretation, uncertain semantics, API decisions, release claims and adopter feedback. |

Reserve two short evidence-review sessions each week. Limit concurrent architecture changes and have workers attach a reproducer, an explanation and the affected-family count to proposed fixes. More generated code is not useful if it increases the owner's unresolved review queue.

### Day-90 acceptance criteria

- Every selected case has a terminal outcome; no unsupported case silently disappears.
- Every published claim names its target profile, DNA mode, comparator and population.
- Oracle instability and infrastructure failures remain separately visible.
- The harness catches deliberately injected false-pass faults.
- The accepted architecture slice and its critical command transcripts pass without unexplained regressions.
- High-impact defects have reproducible causes and retained regressions.
- At least three external users have tested the intended workflow, with repeat use by at least two.
- The next quarter's priorities follow observed adoption blockers and independent failure clusters.

Do not precommit to “99% compatible by day 90.” The expanded baseline may fall sharply as realistic unsupported cases enter. A lower but well-defined score can be more valuable evidence than a higher score on selected caches.

If the oracle-rights question remains unresolved, publish the static corpus inventory, the provenance audit, reference limitations and independently supported semantic/architecture tests. Do not manufacture a live-Excel compatibility claim. Set an explicit strategic review date instead of allowing that dependency to remain indefinitely implicit.

## 9. Decisions and uncertainties to carry forward

The current architecture is sufficiently directed to support this strategy. The next uncertainty to reduce is whether a narrow, demonstrably reliable execution profile solves a recurring problem for identifiable users.

The material unknowns are: the applicable Microsoft agreement and permissible output use; how much of the present 6.4% cached-cell mismatch reflects true engine defects; the number of independent, formula-bearing models available after deduplication; reference stability on effectful or environment-dependent workbooks; actual throughput and unattended SKU cost; and demand for execution versus diagnostics versus a new spreadsheet interface.

The first 90 days should answer those questions with a small useful product and a reproducible body of evidence. The resulting asset is an engine whose supported behaviour can be examined, reproduced and trusted within a clearly stated scope.
