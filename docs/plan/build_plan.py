from pathlib import Path
from collections import defaultdict, Counter
import json, re, hashlib, datetime

ROOT = Path(__file__).resolve().parent
TASKS = []
MILESTONES = {
'm0':'Bootstrap checkpoint — independent lanes, CI and evidence safety',
'm1':'First functional milestone — real Excel open/recalc/edit/save/reopen',
'm2':'Shared formulas and all four skins over one workbook',
'm3':'Workbook-wide references, arrays, names and extension attachment points',
'm4':'Structural editing, transactions and calculation settings',
'm5':'Iterative calculation, data tables, conditional formatting and validation',
'm6':'Excel core UI and complete core document authoring',
'm7':'Conformance expansion, numerical campaigns and measured workloads',
'm8':'Release integration and first-version scope accounting',
}
LIBS = {
'infra':('Infrastructure','xtask','§§1, 10–13','cargo xtask / calcctl; call the same exported library entrypoints as the applications'),
'cfunc':('CalcFunc','calc-func','§§2, 5, 10','calc_func::invoke through the generated catalogue; do not call a detached test implementation'),
'cfml':('CalcFml','calc-fml','§§2, 4, 5','calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps'),
'ceng':('CalcEngine','calc-engine','§§2, 3, 6','calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator'),
'cdoc':('CalcDoc','calc-doc','§§2, 3, 7','calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently'),
'chost':('CalcHost','calc-host','§§2, 3, 8','calc_host::dispatch / step / project / save through a real HostState, not a parallel session'),
'onecalc':('DNA OneCalc','dna-onecalc','§§2, 4, 8','the OneCalc application controller with its in-memory reference provider, never CalcEngine'),
'dnacalc':('DNA Calc','dna-calc','§§8, 9','the shipped skin controller and CalcHost transport; browser tests dispatch real UI events'),
}
PORTS = {
'fresh':'fresh',
'values':'OxFunc/crates/oxfunc_value_types/src/lib.rs: CalcValue, CoreValue, Shared, ReferenceLike and WorksheetErrorCode. Remove the Rc feature arm and string-address payloads; preserve value semantics.',
'func':'OxFunc/crates/oxfunc_core/src/function.rs: FunctionMeta; crates/oxfunc_core/src/function_call.rs: FunctionCallTarget, FunctionCallScratch, FunctionExecutionContextBundle; crates/oxfunc_core/src/registry.rs; crates/oxfunc_core/src/functions/surface_dispatch.rs. Paths after the first are relative to OxFunc/.',
'funcport':'OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.',
'numeric':'OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs: exp, pow_direct, atan and the fyl2x_fn expansion are QUARANTINED, not port sources until provenance clearance. OxFunc/crates/oxfunc_core/src/excel_numeric/ is a discovery location only. Use independent public-ISA/black-box witnesses for replacement code.',
'format':'OxFunc/crates/oxfunc_core/src/locale_format.rs: LocaleFormatContext; OxFml/crates/oxfml_core/src/format/ (resolve exact files in source manifest); OxFml/crates/oxfml_core/src/eval/mod.rs: locale use at function invocation. Do not move formatting into CalcEngine.',
'fml':'OxFml/crates/oxfml_core/src/eval/mod.rs: compiled expressions and evaluation; OxFml/crates/oxfml_core/src/binding/profile.rs: ReferenceNormalFormKey, ReferenceFingerprintPolicy. Discard ReferencePolicy, host-reference syntax and the tree profile.',
'lazy':'OxFunc/crates/oxfunc_core/src/function.rs: FunctionMeta laziness and argument-demand metadata; OxFml/crates/oxfml_core/src/eval/mod.rs: the existing lazy IFS/CHOOSE/SWITCH/IFNA scheduling (OxFml W079, bead fml-kt8.1) as the behavioural reference; OxFml/crates/oxfml_core/src/semantics/ for argument classification. Port the metadata-driven scheduling, not the frame-local Rc/RefCell plumbing.',
'parse':'OxFml/crates/oxfml_core/src/syntax/ and OxFml/crates/oxfml_core/src/language_service/ are discovery paths. The source-manifest bead must resolve exact lexer/parser/service/test files and exported symbols before porting. Preserve lossless source spans; do not copy host-reference productions.',
'refs':'OxCalc/src/oxcalc-core/src/grid/reference_engine.rs: StrictExcelGridReferenceProfile, ExcelGridReferenceSystemProvider; OxFml/crates/oxfml_core/src/binding/profile.rs: reference requests/normal-form identity. Extract Excel semantics, not profiles or BTreeMap<String,...> providers.',
'graph':'OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridAxisValueDependency, GridAxisVisibilityDependency, GridSheetSpanDependency and reverse range indexing. Reimplement with integer IDs in one workbook-wide graph; do not inherit the old per-sheet scheduler.',
'spill':'OxCalc/src/oxcalc-core/src/grid/machine/spill_ledger.rs (spill ownership/blocker design; resolve symbols at source pin); OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridSpillDependency, GridSpillBlockerDependency.',
'engine':'OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.',
'doc':'OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.',
'host':'DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.',
'skins':'DnaTreeCalc/src/dnacalc-skin-ir/ and DnaTreeCalc/src/dnacalc-shell/src/stage.rs: StageSurface. The source-manifest bead resolves exact intent/projection symbols. Replace tree-first WorkspaceState and the monolithic intent enum.',
'gridui':'DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.',
'notebook':'DnaTreeCalc/src/dnacalc-stage-notebook/ (partial implementation; source manifest resolves files); DnaTreeCalc/docs/ux/FRONTEND_UI_DESIGN_AND_ROUTEMAP.md F.2/F.4 (design candidate, not authority). New persistence is specified in plan §8.',
'oracle':'OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.',
'udf':'OxFunc/docs/function-lane/OXFUNC_UDF_REGISTRATION_AND_REGISTRY_MUTATION_CONTRACT.md: registry mutation contract (owner-brief reference); OxFunc/crates/oxfunc_core/src/function_call.rs: execution providers. Port contracts only after confirming code call sites.',
'vba':'OxVba/crates/oxvba-host/src/engine.rs: host callback integration (owner-brief reference). Implement a platform-neutral attachment only; no VBA, COM, MSForms or XLL runtime is built in this phase.',
}

def add(id, title, deps, do, checks, port='fresh', scope=None, priority=1, platform='all', outcome=None):
    prefix, milestone, num = re.fullmatch(r'([a-z]+)-(m\d+)\.(\d+)',id).groups()
    assert prefix in LIBS and milestone in MILESTONES
    lib, package, sections, path = LIBS[prefix]
    deps = deps.split() if isinstance(deps,str) else deps
    # Every task has a structural parent, never a blocking dependency on its epic.
    relations = [{'depends_on_id':f'{prefix}-{milestone}','type':'parent-child'}]
    relations += [{'depends_on_id':d,'type':'blocks'} for d in deps]
    cases = []
    for item in checks.split('||'):
        name, assertion = item.strip().split('=',1)
        name = name.strip()
        assert re.fullmatch('[a-z][a-z0-9_]*', name), (id,name)
        cases.append((f'{id.replace("-","_").replace(".","_")}__{name}',assertion.strip()))
    plat = {'all':'Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.',
            'oracle':'Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.',
            'native':'Windows and Linux x86-64 for the native Exact backend. On wasm (and other architectures) exercise NumericModeUnavailable and retained-vector replay; do not silently select Fast.',
            'tooling':'Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.'}[platform]
    if prefix=='infra':
        command=f'cargo xtask accept --id {id} --platform windows --require-all\ncargo xtask accept --id {id} --platform linux --require-all'
    else:
        command=f'cargo test -p {package} --test acceptance {id.replace("-","_").replace(".","_")} -- --nocapture\ncargo xtask accept --id {id} --platform windows --require-all\ncargo xtask accept --id {id} --platform linux --require-all'
    if platform not in ('tooling',):
        command+=f'\ncargo xtask accept --id {id} --platform wasm --require-all'
    if platform=='oracle': command+=f'\ncargo xtask accept --id {id} --platform excel --require-all'
    checktext='\n'.join(f'- `{n}`: {a}' for n,a in cases)
    outcome = outcome or f'{title.rstrip(".")}. The behaviour is available through the {lib} production path described below, with the named acceptance cases passing.'
    description=f'''## Outcome
{outcome}

## Context
Implements {sections} of DNA_CALC_REIMPLEMENTATION_PLAN.md, within {MILESTONES[milestone]}. Own only {lib} files for this task; consume prerequisite APIs rather than editing another library. Production route: {path}.

## Do
{do}
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

## Port from
{PORTS.get(port,port)}

## Acceptance
```text
{command}
```
{checktext}
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

## Platforms
{plat}

## Out of scope
{scope or 'Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.'}
'''
    TASKS.append(dict(id=id,title=title,issue_type='task',priority=priority,labels=[milestone,prefix,platform],dependencies=relations,description=description))

# Bootstrap: deliberately a checkpoint, not the first functional product milestone.
add('infra-m0.1','Create the monorepo and isolated crate workspaces','',
    'Create one Cargo workspace with calc-value, calc-func, calc-fml, calc-doc-model, calc-doc, calc-engine, calc-host, dna-onecalc, dna-calc, xtask and platform adapter packages. Use one lockfile; every CI job and every agent checkout has its own target directory. Pre-create per-library tests and acceptance-manifest directories so four agents do not edit a shared registry. Commit the plan bundle (this plan, BEAD_GRAPH.md, BEAD_DETAILS.md, the JSONL exports, coverage.json and validate_plan.py) under docs/plan/ so every agent reads the text that bead Context sections cite, and commit the .beads/ tracker. Write AGENTS.md as the agent operating contract: claim a bead with br ready or bv --robot-next and br update <id> --status in_progress; run its acceptance and keep the receipt; one commit per bead on master with the bead ID in the message; br close with the evidence; file a follow-up bead instead of widening scope; all work on master as pull, work on one bead, commit, push, with no branches and no worktrees, ever; an agent working concurrently with another uses its own full clone of the repository on master (which gives it its own target directory), pulls before starting, and resolves a rejected push by pulling master and re-running its acceptance before pushing again; never two cargo invocations in one target directory at the same time; and the honesty rules from the brief (provenance before value, count every fallback, no faked behaviour, no seams without traffic, measure rather than assert).',
    'workspace_builds=Every portable package builds on the native targets and wasm32-unknown-unknown; prohibited OS imports fail the architecture test.||independent_lanes=Four separate clones on master build Func, Fml, Doc and Engine modules concurrently, each in its own target directory, with no shared file writes.||agent_contract=AGENTS.md names the claim, acceptance, commit and close commands, the master-only pull/work/commit/push rule with no branches or worktrees, and the one-cargo-per-target-directory rule, and docs/plan/ contains the plan text and bead exports that the bead descriptions cite.',priority=0,platform='tooling')
add('infra-m0.2','Install Windows Linux and browser CI','infra-m0.1',
    'Add required native test jobs, actual wasm-bindgen/browser test execution in Chromium and Firefox, desktop packaging smoke jobs and source/license checks. Pin toolchains and action revisions; archive logs and artifacts. Do not treat a wasm cargo check as executed tests.',
    'browser_failure_is_red=A deliberately failing browser fixture makes CI fail rather than report a compile-only success.||platform_matrix=Every portable crate and both app cores appear in a machine-readable executed-target manifest.',priority=0,platform='tooling')
add('infra-m0.3','Build the acceptance runner and architecture gates','infra-m0.1',
    'Implement cargo xtask accept --id ID --platform windows|linux|wasm|excel --require-all. Collect per-package manifests; validate test enumeration, run commands, and fail on missing/zero/skipped required cases. Add cargo-metadata dependency gates for OneCalc, skins and platform crates; deny tree-reference tokens and production-only dead seams using a reviewed call-site manifest plus integration coverage.',
    'zero_tests_fail=A misspelled required test name fails the runner.||forbidden_dependency=Adding calc-engine to OneCalc or calc-host to CalcFml is rejected mechanically.',priority=0,platform='tooling')
add('infra-m0.4','Pin legacy sources and resolve inaccessible migration inputs','infra-m0.1',
    'Record commit IDs, working-tree state and blob hashes for every legacy repository under the DnaCalc root (OxFunc, OxFml, OxCalc, OxDoc, DnaTreeCalc, the archived DnaOneCalc, OxReplay, OxXlPlay, ExcelFunctionsHandbook, DnaVisiCalc, Foundation) and the exact files/symbols selected by each port task. All of these are available locally; the 2026-10-12 review found uncommitted changes in OxCalc (consumer.rs and three grid/machine files), OxDoc (oxdoc-xlsx lib.rs plus an untracked cell-add conformance fixture), OxFml (eval/mod.rs) and DnaTreeCalc (.beads only): commit or stash them in those repositories first so every pinned blob is a committed object. Record parser, function-family, corpus and oracle-runner file lists before copying. Mark any module that is genuinely absent as unavailable and route its tasks to fresh implementation.',
    'source_manifest=Every selected port input has a repository, commit, path, content hash, license status and read disposition, and no pinned blob comes from an uncommitted working tree.||missing_source=A port input absent from the pinned manifest is reported as unavailable and never counted as a verified reusable implementation.',priority=0,platform='tooling')
add('infra-m0.5','Quarantine questionable numeric provenance','infra-m0.4',
    'Trace the provenance of OxFunc/crates/oxfunc_core/src/excel_numeric/ without consulting leaked or disassembled sources. x87.rs (890 lines, 21 asm! blocks) states that its EXP/LN/LOG/POWER sequences follow fpw32/tran/i386/87tran.asm and the 87disp.asm control word from the NT CRT source tree, while its ATAN path cites W111 black-box witnesses; mod.rs dispatches x86_64 versus portable; tables.rs with gen_tables.py, gammaln.rs, research.rs and x87_excel_ground_truth.tsv need a per-file disposition. Keep questionable code out of the new tree. Retain an independent black-box witness set with acquisition provenance; create a reviewed disposition for each candidate module. Clearing a comment is not clearance.',
    'quarantine_gate=A source manifest entry marked unresolved or tainted cannot enter the release source allowlist.||independent_witness=Every retained numeric witness links to black-box acquisition metadata and no quarantined implementation is used to generate expected bits.',port='numeric',priority=0,platform='tooling')
add('infra-m0.6','Back up and content-address existing oracle evidence','infra-m0.4',
    'From the owner-accessible checkout inventory OxFunc/smart-fuzzer/cache (1,618 files) and OxFunc/smart-fuzzer/runs (26,774 files), owner-reported at about 3.4 GB, untracked and not git-ignored; hash, deduplicate and copy to a versioned artifact store with a second verified backup. Keep manifests and minimal witnesses in git, bulk blobs outside git. Restore a random manifest-selected sample and record acquisition metadata gaps instead of fabricating them.',
    'restore_roundtrip=Restored blobs hash-identically to the local originals and both backup inventories reconcile.||unknown_metadata=Legacy data without an Excel fingerprint is labelled unknown and cannot certify a new Exact target.',port='oracle',priority=0,platform='tooling')
add('cfunc-m0.1','Define portable values IDs and numerical policy','infra-m0.1',
    'Implement calc-value: workbook/sheet/name/table/node generational IDs, CellKey with u32 row and column, scalar/array/error/reference/callable values, NumericMode and NumericTarget. Use Arc-owned immutable payloads and Send+Sync callable contracts. Checked grid coordinates cover the full Excel bounds without allocating cells.',
    'send_sync=Compile-time assertions prove public immutable values and plan-facing reference tokens are Send+Sync with no Rc opt-out.||grid_edges=A1 and XFD1048576 round-trip; zero/out-of-bounds coordinates fail and distinct sheet IDs cannot alias.',port='values',priority=0)
add('cdoc-m0.1','Define engine-neutral authored operations and revisions','infra-m0.1 cfunc-m0.1',
    'In calc-doc-model define AuthoredRevision, immutable model roots, WorkbookOp batches, structural-edit descriptors, CellInput and document-only metadata. The operation schema contains no engine/session types and never serializes computed cache as authored input. Generate per-capability operation codecs.',
    'op_roundtrip=Every initially admitted operation round-trips through the versioned wire codec.||no_engine_dependency=The model package compiles without CalcEngine, CalcFml or CalcHost and a cached value cannot be decoded as an authored formula replacement.',port='doc',priority=0)

# M1: the first real vertical slice.
add('cfunc-m1.1','Generate the initial function catalogue and call interface','cfunc-m0.1 infra-m0.3',
    'Create a single declarative catalogue that generates IDs, metadata, dispatch, signature help and oracle selection. Add FunctionCallTarget::invoke with caller-owned EvalServices and scratch. Admit arithmetic, comparison, concatenation, SUM and IF metadata for the thin slice; expose lazy argument policy rather than eager-evaluating IF.',
    'catalogue_consistency=Dispatch, help and arity data enumerate exactly the same admitted IDs.||unknown_function=Unknown IDs return a typed registration error without dispatch by a second string switch.',port='func',priority=0)
add('cfunc-m1.2','Port base coercions and scalar arithmetic','cfunc-m1.1 infra-m0.4',
    'Port primitive worksheet coercion, empty versus omitted versus empty-text distinctions, numeric arithmetic, comparison and error propagation through the generated call interface. Preserve signed-zero policy explicitly and pass NumericMode to all arithmetic sites.',
    'mixed_operands=Retained Excel witnesses distinguish direct text/boolean arguments from referenced values and assert exact error codes.||error_order=Competing operand errors return the observed error in the observed order.',port='func',priority=0)
add('cfml-m1.1','Port the lossless Excel lexer and basic parser','infra-m0.1 infra-m0.3 infra-m0.4 cfunc-m0.1',
    'Create the Excel-only syntax tree for literals, unary/binary operators, calls and A1 references; retain whitespace, trivia, incomplete input and UTF-16 editor offsets. Remove host-reference/tree grammar at entry, not as a later profile rejection.',
    'lossless_roundtrip=Formatting, quoted strings and incomplete input reproduce their original source text.||tree_syntax_rejected=A legacy host-reference token gets a precise unsupported-syntax diagnostic and never a tree AST node.',port='parse',priority=0)
add('cfml-m1.2','Implement typed reference binding without the engine','cfml-m1.1 cfunc-m1.1',
    'Bind A1/R1C1 cell and rectangular references against a ReferenceCatalog trait and caller anchor; produce typed relative/absolute operands. Introduce ReferenceReader returning borrowed/chunked values with missing/blank distinctions. OneCalc can implement these traits without an engine.',
    'relative_absolute=Mixed dollar references instantiate correctly at two callers and reject references crossing grid bounds.||provider_reads=An independently instrumented provider sees typed requests, not constructed address strings.',port='refs',priority=0)
add('cfml-m1.3','Expose reusable compiled plans and evaluation frames','cfml-m1.2 cfunc-m1.2',
    'Implement compile, instantiate and evaluate APIs with Arc<CompiledPlan>, caller-owned PlanCache, EvalScratch and per-call EvalFrame. Separate immutable instructions from mutable lexical frames. Route every function through CalcFunc and every reference through the typed reader.',
    'reuse_plan=Two evaluations of one compiled plan produce different values after provider data changes without invoking compile again.||lazy_if=IF does not evaluate the unselected error/volatile branch, as verified by provider-call counters.',port='fml',priority=0)
add('cdoc-m1.1','Implement bounded ZIP package reading and raw preservation','infra-m0.3 cdoc-m0.1 infra-m0.4',
    'Open an .xlsx OPC package into a seekable backing source, a part directory and lazy XML readers. Reject zip-slip, external entity expansion and declared-size bombs under explicit resource limits. Retain untouched compressed payloads and relationship/content-type information.',
    'raw_preservation=An independently read hash of every untouched part payload is unchanged after a no-op save.||malformed_zip=Path traversal and adversarial expansion fail with typed errors and no filesystem writes.',port='doc',priority=0)
add('cdoc-m1.2','Load scalar cells formulas and file-cache provenance','cdoc-m1.1',
    'Parse workbook/sheet relationships, cell addresses, inline/shared strings, numeric/error/boolean cells, basic styles, formulas and cached <v> separately. Materialize only occupied cell records; tag all loaded formula caches FileCached with source-part identity.',
    'authored_vs_cache=A formula whose cached value is deliberately wrong retains the formula and visibly FileCached provenance.||sparse_extent=A workbook containing only XFD1048576 does not allocate a dense worksheet.',port='doc',priority=0)
add('cdoc-m1.3','Author cell changes and write a repair-free thin workbook','cdoc-m1.2',
    'Implement value/formula add, replace and clear for existing sheets; maintain worksheet dimensions, row order, shared-string indices and formula cache invalidation. Build prepare_save and streaming write with a typed refusal path; preserve unsupported untouched parts.',
    'formula_authoring=Add a new formula to a formerly absent cell and independently inspect its XML before Excel reopen.||save_refusal=A refusal produces no partially accepted source image and leaves the caller-owned model revision unchanged.',port='doc',priority=0)
add('ceng-m1.1','Create workbook-wide graph storage and literal input access','infra-m0.3 cfunc-m0.1 cdoc-m0.1',
    'Implement caller-owned EngineState with interned node IDs, one workbook graph, formula-node records and borrowed immutable input pages. Stage calculation-relevant operation effects through prepare_edit. Keep source bytes, formatting and editable cells outside the engine.',
    'two_sheets_one_graph=Nodes on two sheets share one graph and cannot collide by coordinates.||input_sharing=Literal input pages are referenced from the document snapshot without a second owned value copy.',port='graph',priority=0)
add('ceng-m1.2','Calculate the thin slice through CalcFml','ceng-m1.1 cfml-m1.3',
    'Build dependencies for direct references and ranges, compute dirty closure and a deterministic topological ready queue, and evaluate each selected node through CalcFml. Publish a staged ValueDelta with Calculated{revision,tick,mode,target} provenance only after successful finish.',
    'chain_recalculates=Changing a literal recalculates a multi-sheet dependent chain and the observed values match independent fixture expectations.||wrong_cache_not_pass=A wrong file cache is not reused or scored as calculated; injected evaluation failure leaves the previous published generation intact.',port='engine',priority=0)
add('chost-m1.1','Commit authored and engine edits as one transaction','cdoc-m1.3 ceng-m1.2',
    'Create HostState owning the document and EngineState. Translate a command to one WorkbookOp batch; prepare document and engine effects, preflight reservations, then commit both without fallible allocation. Roll back preparation on error; publish one authored revision and a dirty set.',
    'atomic_prepare_failure=Failure after document preparation leaves both original revisions and all cell contents unchanged.||recalc_failure_keeps_edit=A committed edit survives a later recalc failure while old computed values become Stale.',port='host',priority=0)
add('chost-m1.2','Expose open recalc save and delta projections','chost-m1.1',
    'Wire open → document model → engine projection; expose dispatch/step/project/save APIs and a thin headless calcctl caller. Keep save snapshots immutable and clear dirty status only when the acknowledged save revision still equals the current authored revision.',
    'delta_only=A one-cell edit returns only changed cells and affected metadata, verified by serialized delta contents.||concurrent_save_edit=Completing a save of revision r after editing r+1 does not mark r+1 saved.',port='host',priority=0)
add('infra-m1.1','Unify cache and live-oracle verdicts','infra-m0.3 cfunc-m0.1',
    'Implement a canonical CaptureRecord and CellVerdict library shared by file-cache replay and live Excel capture. Track formula denominator, calculated count, correct count, stale/not-modeled/missing/uncertified categories and type-aware comparisons. Decimal <v> is an observation, not a guaranteed binary64 oracle; mark strict-bit certainty separately.',
    'cached_value_trap=A file-cache-only result is counted as not calculated even when numerically equal to the expectation.||denominator_integrity=Hidden sheets, shared formulas and error-valued formulas remain in the enumerated denominator.',port='oracle',priority=0)
add('infra-m1.2','Implement one isolated Excel COM capture adapter','infra-m1.1 infra-m0.4',
    'Consolidate one Windows adapter for opening fixtures with macros/add-ins/external links disabled, setting calculation policy, forcing recalc, capturing raw Value2 number bits and formula/type/settings metadata, saving and reopening. Capture Excel build, compatibility version, CPU, OS, locale, date system and calculation settings. Legacy runners become readers/converters, not alternate capturers.',
    'capture_replays=The same capture produces identical verdict records on Windows, Linux and wasm.||untrusted_workbook=Macro and external-link activation remain disabled and a capture timeout terminates only the isolated worker.',port='oracle',priority=0,platform='oracle')
add('infra-m1.3','Prove the first real workbook round trip','chost-m1.2 infra-m1.2',
    'Add a licensed Excel-authored two-sheet fixture with constants, SUM, a dependency chain, a deliberately perturbed cache and an unrelated opaque part. Run the product HostState/CalcFml path, compare provenance-first, edit a value and add a formula, save, and reopen in real Excel with no repair. Retain input/output packages, raw capture and verdict artifact.',
    'end_to_end=All formula cells in this declared fixture are independently enumerated, actually calculated and compared; the deliberate stale cache is caught.||excel_reopen=Real Excel opens the saved package without repair and calculates the changed and added formulas to the captured expectations.',port='oracle',priority=0,platform='oracle')
add('onecalc-m1.1','Ship the engine-free OneCalc application controller','cfml-m1.3 infra-m0.3',
    'Build OneCalc controller plus a minimal desktop/browser surface with formula text, typed inputs, result, diagnostics and trace. Implement a small typed workbook/sheet/name provider for references; keep CalcEngine and CalcHost absent even from transitive dependencies.',
    'reference_without_engine=A formula reads two explicitly supplied cells and calculates correctly with only CalcFml/CalcFunc.||dependency_gate=cargo metadata for the full OneCalc package graph contains neither calc-engine nor calc-host.',port='fml',priority=0)

# M2: formula sharing and early skins — no dependency on Excel UI completion.
add('cfml-m2.1','Canonicalize R1C1 templates and binding-context keys','cfml-m1.3',
    'Normalize parsed formula structure, not raw strings. Key templates by typed R1C1 offsets/absolutes, lexical names, function/compatibility catalogue generation and semantic syntax settings. Separate binding instantiation keys for sheet/name/table contexts. Do not include the entire authored revision or caller row in template identity.',
    'fill_down_shares=100000 R1C1-equivalent formulas share one parse/compiled template, measured by an independent compile observer.||different_scope_not_alias=Identical text in two sheets with shadowing local names uses correct distinct bindings without losing template sharing.',port='fml',priority=0)
add('cfml-m2.2','Invalidate reusable plans by semantic dependencies','cfml-m2.1',
    'Maintain a caller-owned plan arena and lookup-dependency fingerprints for scoped names, tables, function registry and structural transforms. Evict unreachable plans with explicit ownership; never recompile in evaluate. Bound cache retention by active handles and configurable memory limits without changing semantics.',
    'edit_reuses_plan=A value-only edit preserves plan identity across a full evaluation.||rename_rebinds=Renaming/redefining a referenced scoped name invalidates only relevant binding instances and produces the correct result.',port='fml',priority=0)
add('cfml-m2.3','Compile LET LAMBDA closures and invocation','cfml-m2.1 cfunc-m1.1',
    'Port lexical scope analysis into immutable instructions with frame slots. Distinguish lexical value captures from workbook-name/reference lookup at invocation; expose opaque Arc-backed callable values. Implement an explicit evaluation stack/cancellation checks rather than native stack growth on wasm.',
    'closure_scope=Nested LET/LAMBDA shadowing and reference captures match retained Excel witnesses.||bounded_recursion=Recursive names terminate with the specified resource/error outcome on native and wasm rather than stack overflow.',port='fml',priority=0)
add('cfml-m2.4','Port metadata-driven lazy control forms','cfml-m2.3',
    'Implement IF, IFS, CHOOSE, SWITCH, IFERROR and IFNA evaluation scheduling from each function’s observed metadata; do not assume all branch-like functions short-circuit identically. Preserve array-context branch semantics and error order with corpus witnesses.',
    'branch_observation=Provider call traces match the witnessed evaluation order for each control form.||array_branches=Array predicates and branch shape/error combinations match the retained Excel results.',port='lazy',priority=0)
add('cfml-m2.5','Expose incremental editing and formula-language services','cfml-m2.1',
    'Port completion, signature help, function/name diagnostics and lossless incremental edits into an explicit EditSession. Return UTF-16 spans and cancellation/version tags; reuse the function catalogue and the same binder as final compile.',
    'edit_sequence=Typing, deleting and completing a mixed-reference formula preserves source and the final compiled result.||stale_response=A service reply for an old editor revision is rejected by the consuming controller.',port='parse',priority=0)
add('cfml-m2.6','Own number-format parsing and numeric text policy','cfml-m1.3',
    'Create CalcFml’s number-format AST and formatting service for General, numeric, date/time, text sections, conditions, colors and locale tokens. Define NumberTextService in a lower contract crate so CalcFunc TEXT/value-parsing delegates back without a dependency cycle. Keep editable entry parsing separate from locale-invariant file numeric serialization.',
    'text_consistency=TEXT, formula result display and cell formatting share the same witnessed rounding policy.||file_locale=Comma-decimal UI entry saves locale-invariant numeric XML and reopens unchanged under another locale.',port='format',priority=0)
add('ceng-m2.1','Instantiate shared formula regions without a second evaluator','ceng-m1.2 cfml-m2.2',
    'Store Arc<CompiledPlan> plus caller position/binding IDs per formula or repeated region. Use compressed repeated-formula input regions while preserving per-cell result/dependency identity. Delete any temptation to special-case arithmetic or aggregates in CalcEngine.',
    'shared_region=100000 equivalent fill-down formulas use the public plan path and calculate distinct caller-relative values.||no_side_evaluator=An injected CalcFunc witness is observed identically through OneCalc and engine evaluation; architecture scans reject local operator dispatch.',port='engine',priority=0)
add('cdoc-m2.1','Persist view manifests and stable locator names','cdoc-m1.3',
    'Add a versioned DNA CustomXML manifest, opaque block IDs and hidden internal defined names resolving to visible backing cells/ranges. Store formulas and prose in ordinary sheet cells; store order/tree/canvas/view state only in the manifest. Preserve unknown manifest fields on compatible edits.',
    'manifest_roundtrip=Block IDs, visible formulas, prose and four view layouts survive save/open without a sidecar file.||unknown_version=An unsupported manifest version preserves workbook semantics and produces a recovery warning rather than deleting cells.',port='doc',priority=0)
add('chost-m2.1','Define capability-organized skin intents and subscriptions','chost-m1.2 cdoc-m2.1',
    'Replace the tree-first projection with Workbook, Selection, Formula, BlockLayout, Object, Diagnostics and Viewport capability APIs. Provide schema-version negotiation, receipts, stable object IDs and range/dependency/block subscriptions. Drive the contract with all four skin controllers from its first public version.',
    'four_clients=Grid, notebook, tree and canvas controllers issue commands through the same dispatcher against one HostState.||projection_isolation=Changing a canvas position changes view metadata only and triggers no formula rewrite or recalc.',port='skins',priority=0)
add('chost-m2.2','Implement canonical commands and undo journal','chost-m2.1',
    'Create generated command IDs, argument schemas, capability checks and command receipts. Record inverse authored patches and view changes in one transaction journal; coalesce text-edit commits, paste and fill according to explicit gesture boundaries. Separate editor-buffer undo from workbook undo.',
    'undo_redo=An edit made in one skin is undone and redone from another with matching authored and engine revisions.||batch_granularity=A multi-cell paste has one undo step; cancelled editor typing has none.',port='host',priority=0)
add('chost-m2.3','Implement block editing on ordinary worksheet anchors','chost-m2.2',
    'Create, move, reorder, delete and alias a block using ordinary cells, internal locator names and manifest changes in one operation batch. Keep view reorder distinct from physical worksheet move. Expose explicit reserve-space commands for spill outputs; never silently relocate neighboring formulas.',
    'reorder_preserves_formula=Notebook/tree view reorder does not change formula identity or Excel references.||physical_move=The explicit move-cells command uses CalcFml structural reference transforms and records one undo transaction.',port='notebook',priority=0)
add('chost-m2.4','Reconcile non-Excel layouts after real Excel edits','chost-m2.3',
    'Resolve block locator names after row insertion, defined-name rename, sheet rename and deletion. Use explicit orphan/ambiguous statuses; never guess between duplicate targets. If manifest is stripped, present ordinary workbook cells and offer a deterministic derived layout without inventing recovered prose.',
    'excel_row_insert=Locator names updated by Excel keep the same block IDs attached to the intended cells.||deleted_sheet=Deleting the backing sheet yields orphan metadata and #REF witnesses, not resurrected numeric values.',port='notebook',priority=0)
add('dnacalc-m2.1','Build shared shell and desktop/browser transport','chost-m2.1 infra-m0.2',
    'Keep Leptos for UI and Tauri for desktop packaging. Browser HostState lives in one worker; desktop HostState lives on one native owner thread. Use the same versioned transport, cancellation and delta protocol. Keep viewport rendering responsive while recalc advances in bounded cooperative steps.',
    'same_receipts=The same operation log produces identical receipts and values over native and browser transports.||edit_during_calc=UI selection/scroll events remain serviceable during recalc and a newer edit cancels/restarts publication safely.',port='skins',priority=0)
add('dnacalc-m2.2','Implement the notebook skin editor','dnacalc-m2.1 chost-m2.3 cfml-m2.5',
    'Build virtualized code/prose blocks with formula editing, output/error/provenance display, insert/delete/reorder, markdown preview and keyboard traversal. Run a cell requests calculation of its Excel dependencies; visual order is not an execution kernel. Read/write only Host commands.',
    'notebook_product=Typing a formula into a notebook block updates an ordinary backing cell and a dependent grid cell.||prose_roundtrip=Markdown source and order survive xlsx save/reopen and remain readable in real Excel.',port='notebook',priority=0)
add('dnacalc-m2.3','Implement the tree editor skin','dnacalc-m2.1 chost-m2.3 cfml-m2.5',
    'Build a virtualized outline with folders, blocks, inline labels/formulas, collapse/expand, drag reparenting and dependency navigation. Parent/child is view metadata only; references always use Excel names/addresses. Show values, errors, provenance and spill ownership alongside nodes.',
    'reparent_is_view=Moving a node to another parent changes no formula or calculation result.||cycle_in_layout=A drag that would create a UI hierarchy cycle is rejected without affecting the workbook graph.',port='skins',priority=0)
add('dnacalc-m2.4','Implement the free-form canvas skin','dnacalc-m2.1 chost-m2.3 cfml-m2.5',
    'Build a culled 2D scene of formula/prose cards with pan/zoom, drag/resize, selection and keyboard-accessible card traversal. Dependency lines are derived views; creating a formula link invokes a host formula-edit command. Store positions in manifest coordinates independent of screen pixels.',
    'canvas_edit=Editing a card changes its real worksheet formula and all other skins receive its result delta.||layout_only=Moving/resizing 100 cards produces only layout operations and no evaluation calls.',port='skins',priority=0)
add('dnacalc-m2.5','Implement the Excel skin minimum grid and formula bar','dnacalc-m2.1 cfml-m2.5',
    'Port pure grid geometry/RenderPlan and one DOM editor; add visible-window subscriptions, formula bar and name box. Render provenance, errors and basic spill outlines. Use host command IDs and shared selection state, not a second grid model.',
    'real_grid_edit=A typed cell entry reaches HostState and redraws only subscribed changed cells.||far_scroll=Navigate to XFD1048576 with bounded viewport data rather than a full-sheet DOM or dense cell allocation.',port='gridui',priority=0)
add('dnacalc-m2.6','Show callables spills and dependency navigation in every skin','dnacalc-m2.2 dnacalc-m2.3 dnacalc-m2.4 dnacalc-m2.5 cfml-m2.3',
    'Use shared formula detail panels for precedents/dependents, source and evaluated value. A raw LAMBDA cell retains Excel’s uninvoked-cell error presentation; offer a named-LAMBDA definition and explicit invocation inspector without inventing a non-Excel reference language. Show spill anchor versus child ownership and blockers.',
    'lambda_truth=The same raw/named/invoked LAMBDA workbook has consistent Excel cell values and explicit callable inspection in all skins.||dependency_jump=Selecting a precedent navigates to its actual workbook anchor across skins.',port='skins',priority=0)
add('onecalc-m2.1','Add OneCalc editing traces formats and callable fixtures','onecalc-m1.1 cfml-m2.3 cfml-m2.5 cfml-m2.6',
    'Wire the shared formula-language service, formatting pane, callable invocation controls and typed provider fixture editor into OneCalc. Permit multi-sheet reference examples through a trivial provider without adopting a dependency graph.',
    'complete_formula_path=LET/LAMBDA, error handling and TEXT run through the same public API as the engine.||provider_not_engine=Changing a fixture re-evaluates the selected formula directly and the dependency gate still excludes CalcEngine.',port='fml',priority=0)
add('infra-m2.1','Prove cross-skin xlsx persistence with Excel edits','dnacalc-m2.6 chost-m2.4 infra-m1.2',
    'Create an Excel-interchange suite covering all three persistence candidates for comparison, with the chosen hybrid as production. Exercise two skins concurrently, row insertion, alias/locator rename, backing-sheet deletion, cross-block names, raw/named lambdas and spills. Include a 10000-block notebook and retain size/memory/open-time measurements without targets.',
    'interchange_matrix=Every requested persistence case has retained pre/post Excel packages and expected recovery states.||ten_thousand_blocks=All 10000 block IDs reconcile and formulas remain ordinary Excel cells; measured overhead is reported rather than declared fast.',port='oracle',priority=0,platform='oracle')
# M3 CalcFunc: mechanically port bounded families, then enforce catalogue closure.
FAMILIES = [
('Port text and Unicode functions','CONCAT, CONCATENATE, TEXTJOIN, LEFT, RIGHT, MID, LEN, FIND, SEARCH, REPLACE, SUBSTITUTE, TRIM, CLEAN, UNICODE, UNICHAR, UPPER, LOWER and PROPER', 'unicode_units=Surrogate pairs, combining marks, embedded nulls and compatibility-version differences match retained witnesses.||text_errors=Empty/missing/reference arguments preserve the observed coercions and errors.'),
('Port numeric rounding and integer functions','ABS, SIGN, INT, TRUNC, ROUND, ROUNDUP, ROUNDDOWN, MROUND, CEILING, FLOOR, MOD, QUOTIENT, EVEN and ODD', 'rounding_boundaries=Positive/negative half cases and signed zero follow the selected mode and domain policy.||large_integers=Inputs around binary64 integer boundaries do not acquire integer overflow or host-specific casts.'),
('Port transcendental and trigonometric adapters','EXP, LN, LOG, LOG10, POWER, SQRT, trigonometric and hyperbolic function catalogue entries; call NumericBackend only and do not copy quarantined sequences', 'backend_called=A recording numeric backend observes every sensitive operation for this family.||domain_errors=Zero, negative, overflow and near-domain-boundary inputs retain worksheet error semantics in both modes.'),
('Port aggregate and counting functions','SUM, PRODUCT, SUMSQ, SUMPRODUCT, COUNT, COUNTA, COUNTBLANK, AVERAGE, MIN, MAX and their A-suffixed variants', 'sparse_blanks=Whole-column references preserve blank cardinality without materializing every cell.||argument_classes=Direct logical/text values and values read from ranges follow their distinct rules.'),
('Port conditional aggregates and criteria matching','SUMIF, SUMIFS, COUNTIF, COUNTIFS, AVERAGEIF, AVERAGEIFS, MAXIFS and MINIFS', 'criteria_semantics=Wildcards, escape characters, numeric/text criteria and aligned range sizes match Excel fixtures.||mismatched_shapes=Unequal criteria ranges return observed errors rather than silently truncate.'),
('Port lookup and reference-returning functions','INDEX, MATCH, XMATCH, XLOOKUP, VLOOKUP, HLOOKUP, LOOKUP, CHOOSE, ROW, COLUMN, ROWS, COLUMNS and AREAS', 'reference_result=Reference-returning INDEX remains a reference until the consuming context dereferences it.||lookup_edges=Duplicate keys, search direction, approximate boundaries and not-found defaults match witnesses.'),
('Port date and time functions','DATE, TIME, YEAR, MONTH, DAY, HOUR, MINUTE, SECOND, DATEVALUE, TIMEVALUE, DAYS, DAYS360, EDATE and EOMONTH', 'date_systems=1900 leap-year compatibility and 1904 offsets use the supplied workbook date system.||locale_parse=Date/time text parsing uses the supplied locale and never the process locale.'),
('Port business-calendar functions','WEEKDAY, WEEKNUM, ISOWEEKNUM, NETWORKDAYS, NETWORKDAYS.INTL, WORKDAY, WORKDAY.INTL and YEARFRAC', 'calendar_edges=Weekend masks, leap boundaries and duplicate holidays match oracle cases.||invalid_calendar=Invalid weekend codes and date bounds produce the captured errors.'),
('Port distribution functions in bounded dispatch batches','Normal, lognormal, beta, gamma, chi-square, Student t, F, binomial, negative-binomial, Poisson, Weibull, exponential and hypergeometric catalogue entries; copy semantics/tests unchanged except backend plumbing', 'tails_and_inverse=Interior, tail and inverse-roundtrip witnesses are preserved with mode-labelled numerical verdicts.||distribution_domains=Invalid probabilities and shape parameters return exact worksheet error types.'),
('Port descriptive statistics and ranking functions','MEDIAN, MODE, percentile/quartile families, RANK, LARGE, SMALL, variance/stdev families, skew/kurtosis and geometric/harmonic means', 'statistical_coercion=Empty, text, error and logical array inputs match the established coercion matrix.||sample_edges=Zero/one-element and tied-rank cases match independent expectations.'),
('Port regression correlation and forecast functions','CORREL, PEARSON, COVARIANCE, SLOPE, INTERCEPT, RSQ, STEYX, LINEST, LOGEST, TREND, GROWTH, FORECAST and ETS entries', 'regression_shapes=Multi-variable and constant-column cases produce captured array shapes and values.||ill_conditioned=Singular/ill-conditioned witnesses are retained and classified rather than tolerance-laundered.'),
('Port financial cash-flow functions','PV, FV, PMT, NPER, RATE, IPMT, PPMT, CUMIPMT, CUMPRINC, NPV, IRR, MIRR, XNPV, XIRR and RRI', 'solver_branches=Multiple-root, no-root and near-zero-rate cases match captured convergence/error behaviour.||cashflow_dates=Unsorted and repeated dates use the correct documented/captured validation order.'),
('Port securities and depreciation functions','Bond price/yield/duration, coupon-date/day families, accrued interest, discount, treasury bills, SLN, SYD, DB, DDB, VDB and AMOR entries', 'basis_dates=All admitted day-count bases and coupon boundaries have mode-labelled witnesses.||depreciation_edges=Partial periods and invalid cost/salvage/life inputs preserve expected errors.'),
('Port complex and engineering functions','Complex IM families, CONVERT, BESSEL, ERF/ERFC, DELTA, GESTEP and base-conversion/bitwise functions', 'complex_branches=Complex signs, suffixes and branch-cut witnesses match the declared numeric target.||conversion_bounds=Bit widths, base ranges and unit incompatibilities are validated without host-integer overflow.'),
('Port database and filtering aggregates','D-suffixed database functions, SUBTOTAL and AGGREGATE', 'visibility_dependencies=Filtered versus manually hidden rows yield the correct values and dependency reads.||database_criteria=Repeated field criteria and text wildcard cases retain the observed database semantics.'),
('Port dynamic array transformation functions','FILTER, SORT, SORTBY, UNIQUE, SEQUENCE, RANDARRAY, TRANSPOSE, TAKE, DROP, CHOOSECOLS, CHOOSEROWS, HSTACK, VSTACK, TOCOL, TOROW, WRAPROWS, WRAPCOLS and EXPAND', 'array_shapes=Empty, zero-length, padding, ordering and error cases preserve Excel shapes and errors.||lazy_source=Sparse/reference sources are consumed through the typed reader rather than eagerly copied by an adapter.'),
('Port callable array helpers','MAP, REDUCE, SCAN, BYROW, BYCOL, MAKEARRAY and ISOMITTED', 'callback_contract=Invocation count, argument shapes and callback error order match observed helper semantics.||nested_callables=Nested helper lambdas do not share mutable frames or leak captures across evaluations.'),
('Port information and host-context functions','IS families, TYPE, N, NA, ERROR.TYPE, CELL, INFO, SHEET, SHEETS and FORMULATEXT', 'host_facts=Each host-sensitive result comes from a declared typed provider dependency.||unavailable_facts=Missing platform facts return a typed unavailable result rather than an invented computer/environment value.'),
('Port volatile and provider-backed function adapters','NOW, TODAY, RAND, RANDBETWEEN, RTD, external/CUBE/link/data/image/catalogue adapters already present in the pinned source', 'replay_providers=Fixed time/random/external tapes reproduce the same outputs and read facts across targets.||missing_provider=Unavailable services are reported and counted, never answered from file-cache values.'),
('Port modern text and locale-sensitive catalogue additions','TEXTSPLIT, TEXTBEFORE, TEXTAFTER, regex entries and all locale/version-sensitive text additions present in the pinned catalogue', 'compatibility_version=The same witnessed input is evaluated under each admitted compatibility version with recorded differences.||catalogue_presence=Entries absent from the pinned catalogue are reported as absent rather than fabricated from a later Excel version.'),
('Port remaining logical and compatibility aliases','AND, OR, XOR, NOT, TRUE, FALSE, IFNA and all legacy/compatibility aliases in the pinned catalogue; lazy scheduling remains owned by CalcFml metadata', 'alias_identity=Aliases resolve to the canonical declaration and share help, arity and invocation semantics.||logical_arguments=Array/reference/direct arguments obey the same witnessed coercion order as Excel.'),
]
for i,(title, family, checks) in enumerate(FAMILIES,1):
    deps='cfunc-m1.2 infra-m0.4'
    if i in (3,9,10,11,12,13,14): deps+=' cfunc-m3.23'
    if i==17: deps+=' cfml-m2.3'
    add(f'cfunc-m3.{i}',title,deps,
        f'Port the named family only: {family}. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.',checks,port='funcport')
add('cfunc-m3.22','Declare numerical sensitivity and generate backend routing','cfunc-m1.1',
    'Add metadata for invariant, arithmetic-order, transcendental, reduction and numeric-text sensitivity. Generate resolved per-mode call handles from it, and derive sensitivity through helper dependencies. Forbid ad-hoc function-name/mode branches in CalcFml or CalcEngine.',
    'sensitivity_closure=Every admitted function/operator has one explicit numeric policy and helper dependencies cannot hide a sensitive call.||mode_routes=A recording backend demonstrates generated dispatch selects the requested mode without a string switch.',port='func')
add('cfunc-m3.23','Implement the deterministic portable Fast backend','cfunc-m3.22',
    'Pin a portable Rust math implementation and compiler flags; forbid fast-math and uncontrolled FMA contraction. Standardize rounding/subnormal/error handling and use deterministic reduction order. Label Fast distinct from Exact; make unsupported Exact an explicit error rather than a fallback.',
    'cross_target_bits=Recorded Fast vectors have identical output bits on Windows, Linux and wasm under the pinned implementation.||no_exact_fallback=Requesting an unavailable Exact target fails before calculation begins.',port='numeric',priority=0)
add('cfunc-m3.24','Implement a reviewed native Exact instruction adapter','cfunc-m3.22 infra-m0.5',
    'After provenance clearance, implement the minimal x87 wrapper from public ISA documentation and independently derived black-box arithmetic graphs, in a dedicated cfg-gated backend crate. Restore control/status policy correctly and contain unsafe code. Select only a recorded Excel-build/CPU numerical target; do not port unresolved NT-derived material.',
    'native_witness_bits=All admitted independent Exact witnesses match the selected target bit-for-bit.||fpu_environment=Calls preserve the caller floating-point control environment and repeated calls do not leak x87 stack state.',port='numeric',platform='native')
add('cfunc-m3.25','Audit complete catalogue migration and generated outputs',''+' '.join(f'cfunc-m3.{i}' for i in range(1,25)),
    'Compare the pinned old catalogue with the new declaration set, including operators, aliases and hidden/internal worksheet surfaces. Require a one-to-one disposition for every ID and no missing implemented function. Generate help/ExcelFunctionsHandbook/XLL-tooling tables from the same declaration; report unverified/divergent functions honestly.',
    'catalogue_bijection=Every source ID maps to an implementation or a named explicit unsupported disposition; no silent omission exists.||derived_paths=Changing one declaration regenerates matching dispatch, metadata, help and verification tables.',port='func')

# M3 CalcFml reference semantics and graph consumers.
add('cfml-m3.1','Bind scoped and caller-relative defined names','cfml-m2.2',
    'Resolve workbook and sheet-local names, shadowing, forward names and relative name formulas against an explicit invocation anchor. Share name definition plans but key evaluated name instances by caller context when required. Keep callable names and reference-valued names typed.',
    'name_shadowing=Sheet-local names shadow workbook names without cross-sheet cache aliasing.||relative_name=One relative name invoked from two cells reads different intended references and recalculates after a definition edit.',port='refs')
add('cfml-m3.2','Bind structured tables and 3D sheet spans','cfml-m3.1',
    'Compile table sections, escaped column labels, row-context references and 3D sheet spans to IDs and relative operands. Sheet-span membership is supplied from current sheet order, not frozen during parse. Preserve unsupported function/reference combinations as Excel errors or typed capability gaps.',
    'table_context=This-row and headers/totals references resolve correctly in and outside a table.||span_lifecycle=Moving/adding/deleting a sheet changes 3D membership exactly as the recorded structural witness requires.',port='refs')
add('cfml-m3.3','Report dynamic references and reference algebra reads','cfml-m3.2',
    'Implement range, union, intersection, spill and implicit-intersection operators; route INDIRECT/OFFSET text resolution through CalcFml’s Excel binder. Return dynamic read footprints, including failed lookup/absence watches, without owning the graph.',
    'dynamic_switch=Changing INDIRECT text produces a replacement read footprint and the correct value.||absent_target=A currently invalid name/sheet reference records enough identity dependency to become valid after the missing target is created.',port='refs')
add('cfml-m3.4','Preserve array evaluation and legacy array contexts','cfml-m2.4 cfunc-m3.16',
    'Implement scalar, dynamic-array and legacy-CSE evaluation contexts, broadcasting, omitted arguments, nested arrays and array-valued errors. Keep cell publication separate from callable/raw formula values and declare allocation limits as observable resource outcomes.',
    'legacy_cse=The same expression in legacy array and dynamic-array contexts matches captured shape/intersection differences.||array_limits=A resource limit is typed and counted, never a materialization cap returning a cached answer.',port='fml')
add('cfml-m3.5','Bind external workbook references through a typed catalogue','cfml-m3.2',
    'Add external book IDs, link versions and open/closed book resolution without filesystem/network access in CalcFml. Return requested external reference facts for the provider. Preserve workbook/sheet name quoting and canonical file formula serialization.',
    'external_identity=Two links to similarly named workbooks remain distinct by link identity.||unresolved_link=An unavailable external target produces a typed missing dependency with no silent use of an old file cache.',port='refs')
add('ceng-m3.1','Index ranges whole axes and absence dependencies','ceng-m2.1 cfml-m3.3',
    'Implement compressed rectangle and whole-row/column reverse dependency indices over interned IDs. Include value, shape, existence, visibility, name and metadata facets. Use point/range intersection queries and sparse cursors; do not expand whole axes into per-cell edges.',
    'whole_column=Editing a populated or newly created cell in a whole-column dependency dirties the right dependents.||compressed_storage=The number of stored dependency records does not grow with the number of blank cells in a referenced whole sheet.',port='graph')
add('ceng-m3.2','Schedule scoped name instances and forward lookups','ceng-m3.1 cfml-m3.1',
    'Represent names as definition/lookup nodes plus caller-specific instances when relative. Track definition edits, rename and undefined-to-defined transitions. Build cell/name cycles in the same graph, not a separate name evaluator.',
    'relative_names=Two callers of one relative name calculate independently and invalidate on their actual inputs.||name_cycle=Cell-name-name cycles are detected as one workbook SCC, not an arbitrary recursion timeout.',port='graph')
add('ceng-m3.3','Update dynamic read overlays atomically','ceng-m3.1 cfml-m3.3',
    'Commit observed read footprints only with their evaluation result. Replace obsolete dynamic edges; retain conservative watches for unresolved lookups. If new reads expose an uncalculated prerequisite or a cycle, reschedule/reclassify before publication and report the reason.',
    'switch_removes_edge=After INDIRECT changes targets, edits to the old target no longer dirty the dependent.||late_prerequisite=An evaluation that discovers a dirty prerequisite cannot publish a value based on stale input.',port='graph')
add('ceng-m3.4','Implement spill allocation and blocker invalidation','ceng-m3.3 cfml-m3.4',
    'Port spill ownership/extent/blocker concepts into a transactional ledger. Detect occupied cells, merged cells, table conflicts, sheet boundaries and competing spills; publish anchor errors and remove obsolete children atomically. Index blockers even for failed spills.',
    'unblock_spill=Clearing a blocker recalculates the anchor and publishes the complete new extent.||shrink_overlap=Shrinking or competing spills never leave orphan children or overwrite authored cells.',port='spill')
add('ceng-m3.5','Calculate sheet spans and table-structure dependencies','ceng-m3.2 cfml-m3.2',
    'Add table membership/column/header/totals nodes and sheet-order/span nodes to the workbook graph. Expand values lazily in current order; invalidate on schema changes even when numeric cells are unchanged. Maintain row-visibility facets for SUBTOTAL/AGGREGATE.',
    'span_move=Reordering sheets changes a 3D result without rebuilding independent sheets.||table_resize=Adding a table row or renaming a column updates dependent formulas and preserves stable table IDs.',port='graph')
add('ceng-m3.6','Replay volatile time and random effects per tick','ceng-m3.3',
    'Provide explicit tick/time/random effect tapes, node-stable RNG streams and invocation order. Distinguish volatility invalidation from external/RTD updates. Permit oracle capture injection without claiming the deterministic local RNG reproduces Excel’s random generator.',
    'tick_replay=The same snapshot and effect tape produce identical values on all targets.||volatile_scope=A volatile function recalculates under the requested trigger while an RTD-only change follows its own affected nodes.',port='engine')
add('ceng-m3.7','Resolve external workbook snapshots and link updates','ceng-m3.3 cfml-m3.5',
    'Use versioned external input snapshots supplied by the host, with an explicit trust/provenance policy. Support linked open workbooks through host-coordinated operations while each workbook retains one graph; record unavailable/closed-link cached observations distinctly from calculated values.',
    'link_update=A versioned external value update invalidates only its dependent graph region.||stale_external=Results that depend on untrusted stale external observations cannot receive clean Calculated provenance.',port='engine')
add('cdoc-m3.1','Read and write workbook and sheet defined names','cdoc-m2.1',
    'Implement scoped defined names, relative formulas, hidden/internal names and built-in print/filter names with stable local IDs. Preserve source spelling and formula text, and author additions/deletions without mixing manifest locator names with user aliases.',
    'scoped_roundtrip=Workbook and sheet-local duplicate name spellings survive independently.||name_authoring=Adding a named LAMBDA and deleting a normal name yields repair-free workbook XML.',port='doc')
add('cdoc-m3.2','Read and write structured table definitions','cdoc-m3.1',
    'Model table IDs, display names, ranges, columns, calculated/totals formulas, filters and style data; maintain relationships and table parts on create/resize/delete. Keep table metadata separate from computed results.',
    'table_roundtrip=Create, resize and rename a table then reopen in Excel without repair.||table_identity=Column reorder preserves stable identity and dependent formulas are updated only through host operations.',port='doc')
add('cdoc-m3.3','Preserve shared array and dynamic-array formula encodings','cdoc-m1.3 cfml-m3.4',
    'Read shared formula groups, legacy array ranges, dynamic array metadata and cached children without confusing authored and derived cells. Write changed groups by splitting/reindexing or expanding lawful formulas; never serialize a spill child as an independent authored formula.',
    'shared_group_edit=Editing one member preserves correct formulas in all other group members after Excel reopen.||spill_save=The anchor formula, spill metadata and cached output shape reopen consistently without duplicate formulas.',port='doc')
add('cdoc-m3.4','Preserve external links richData and cell metadata','cdoc-m1.3',
    'Model external-link relationships and preserve richData, cm/vm metadata and unknown extension relationships with a fidelity ledger. Update references only for supported edits; refuse edits that cannot preserve required cross-part consistency.',
    'metadata_roundtrip=Untouched richData and external-link payloads retain their hashes and relationship targets.||unsafe_rewrite=An edit requiring an unsupported metadata rewrite refuses explicitly rather than dropping metadata.',port='doc')
add('cdoc-m3.5','Model chart drawing pivot and page-setup attachment data','cdoc-m3.2',
    'Define engine-neutral object IDs, anchor geometry, chart-series formula locations, pivot-source/cache descriptors and print/page metadata. Keep unknown parts losslessly; provide authoritative edit/relationship slots for later area modules without implementing rendering or refresh.',
    'object_registry=An opened workbook exposes real chart/pivot/page objects with resolvable IDs and referenced source ranges.||untouched_objects=A scalar edit elsewhere preserves their untouched payloads and relationship graph.',port='doc')
add('ceng-m3.8','Attach generic derived participants to the workbook graph','ceng-m3.5 cdoc-m3.5',
    'Introduce sealed production participant kinds for chart-series reads, pivot source/cache revisions and layout-derived reads using one graph. Expose invalidation/evaluation hooks as data, not a second scheduler. A production dependency inspector and host object registry consume the hooks now.',
    'series_invalidates=Editing a referenced cell marks the actual chart-series participant dirty.||pivot_source_invalidates=A table membership change invalidates a pivot-source participant without pretending the pivot has been refreshed.',port='graph')
add('chost-m3.1','Publish calculation generations and object revisions','chost-m2.4 ceng-m3.8 ceng-m3.4',
    'Publish value/spill/dependency/object deltas with authored and calculation revisions. Add subscriptions for block anchors, ranges, chart-series and pivot-source objects. Reject stale worker responses and keep all skins on a coherent publication generation.',
    'stale_tick=An old recalc completion cannot overwrite a newer edit’s projection.||object_deltas=An object source change sends the affected object revision and value deltas without rereading whole sheets.',port='host')
add('chost-m3.2','Expose generic refresh and render attachment interfaces','chost-m3.1',
    'Define host-owned refresh requests, transactional pivot output patches, immutable chart datasets and print snapshots. The production inspector reports pending/unsupported derived object state honestly; rendering/refresh adapters will be installed by separable-area implementation beads.',
    'refresh_not_faked=A refresh request without an implementation is a visible unsupported result, not a successful no-op.||snapshot_revision=An object snapshot labels the precise authored/calculated/source revisions used.',port='host')

# Separable-area planning tasks: after initial engine and attachment traffic, never after skin completion.
AREAS = [
('Charting and drawing objects','Chart types (including combination/secondary axes and modern families), series evaluation, formatting, themes, labels, legends, shapes/images, anchors, hit-testing and accessibility. Do not implement pivots or printing here.','cdoc-m3.5 ceng-m3.8 chost-m3.2','chart_plan_slots=Every inventoried chart/drawing command maps to a future implementation bead and the plan names dataset/geometry/format APIs.||chart_attachment=Run the existing production dependency-inspector scenario to prove source edits reach chart participants.'),
('Pivot tables caches and GETPIVOTDATA','Cache records, grouping, calculated fields/items, layout/subtotals/totals, refresh, pivot charts, slicers and GETPIVOTDATA; distinguish worksheet-source and external/OLAP/data-model sources. Do not claim generic cached preservation is refresh.','cdoc-m3.5 ceng-m3.8 chost-m3.2','pivot_plan_slots=The imported implementation subgraph covers cache, layout, refresh and GETPIVOTDATA with real Excel witnesses.||pivot_attachment=The production source-edit scenario dirties the cache participant and rejects false refresh completion.'),
('Printing page layout and export','Page setup, margins, paper, orientation, breaks, scaling, repeated titles, headers/footers, print area, charts and font metrics; PDF/export and native/browser print adapters. Do not replace workbook screen layout.','cdoc-m3.5 chost-m3.2','print_plan_slots=Every Page Layout/print command has an implementation bead and font/geometry risks have measured probes.||print_attachment=The production print-snapshot command exports consistent page setup and revision-labelled content.'),
('External data connections and analytical integrations','Import/query/connection refresh, linked data types, CUBE/OLAP and data-model-backed pivot sources; credentials/trust/cancellation/cache provenance; statistical/what-if tools such as Goal Seek and Solver surface inventory. Core external workbook links and TABLE() remain core, not deferred here.','ceng-m3.7 chost-m3.2','data_plan_slots=All inventoried external/analytic commands map to a concrete provider/implementation bead or explicitly phase-2 add-in placement.||external_attachment=The existing production external-update path carries version/provenance and does not silently fetch network data.'),
('Extended desktop command and dialog surface','Reconcile the captured reference Excel edition against the full command inventory: advanced/custom UI, accessibility tools, review/history/sharing, collaboration/service entrypoints and remaining dialogs. Core Format Cells, names, CF/DV, sorting/filtering, protection and formula tools stay in core beads. No inventory gap may be relabelled complete by a disabled button.','chost-m3.2 infra-m3.1','ui_tail_plan=The remaining desktop commands have individually owned implementation beads and explicit platform constraints.||inventory_no_holes=The union of core, separable and declared phase-2 inventory entries equals the captured command set, with no missing category.'),
]
for i,(name,do,deps,checks) in enumerate(AREAS,20):
    add(f'infra-m3.{i}',f'Plan {name.lower()} implementation',deps,
        f'Produce this area’s executable implementation subgraph, not implementation code: {do} Record scope, non-scope, core dependencies, exact attachment API consumers, late-discovery risks and required Excel captures. Validate and import child beads before marking the area planning task complete. Add those children as blockers of the first-version release gate; this planning bead is not completion of the feature.',checks,port='fresh',platform='tooling',scope='Implementing this separable area or deferring any notebook/tree/canvas work. This is the one allowed detailed-planning bead for this area.')
add('infra-m3.1','Capture and reconcile the complete reference Excel UI inventory','infra-m1.2 chost-m2.2',
    'Pin the reference desktop edition/build, then inventory ribbon/backstage/quick-access/contextual tabs, all dialogs and keyboard commands using public documentation and black-box inspection. Record stable command-family IDs, platform availability and done/partial/not-started status backed by tests. Reconcile every extra entry with a core bead or one separable-area planning bead; never assume the prompt’s list is exhaustive.',
    'inventory_reconciles=Every captured command belongs to one mapped family and every done entry points to an executed interaction/headless test.||reserved_not_done=A phase-2 or separable-area placeholder cannot be marked done merely because its button exists.',port='skins',priority=0,platform='oracle')
add('infra-m3.2','Prove the unified workbook engine milestone','ceng-m3.4 ceng-m3.5 ceng-m3.6 ceng-m3.7 cfml-m3.4 chost-m3.1 infra-m1.2',
    'Run a retained feature matrix with multi-sheet names, 3D ranges, tables, dynamic references, spill blockers, external snapshots and legacy arrays through HostState. Compare independent captures and assert there is one evaluator route and one graph per workbook.',
    'workbook_matrix=Every required feature case is actually calculated and its exact/error/shape verdict retained.||no_hidden_lane=Production call instrumentation demonstrates all formulas reached CalcFml, including repeated R1C1 regions.',port='oracle',platform='oracle')
# M4: structural mutation and transaction semantics.
add('cfml-m4.1','Transform references for copy fill move and paste','cfml-m3.3',
    'Implement pure structural transforms over reference operands and source spans: copy versus cut/move, mixed absolute references, cross-sheet paste, discontiguous areas and formula reconstruction. Return affected binding/template facts; do not rewrite formulas by regex.',
    'copy_vs_move=Captured Excel cases distinguish relative adjustment on copy from target-following on cut/move.||mixed_anchors=Filling formulas containing absolute rows/columns, names and quoted sheets preserves intended targets.',port='refs')
add('cfml-m4.2','Transform references for cell axis and sheet lifecycle edits','cfml-m4.1',
    'Handle insert/delete cells, rows and columns; sheet add/rename/move/delete; range shrink/expansion, #REF tombstones and 3D endpoint rules. Apply the same transformer to cell formulas, names, CF/DV formulas and chart-series formula slots.',
    'delete_reference=Deleting part/all of a referenced area produces the witnessed shrink or #REF form.||noncell_formulas=One sheet rename updates name, rule and series references using the same semantic transform.',port='refs')
add('cdoc-m4.1','Implement persistent sparse pages and axis order maps','cdoc-m3.3',
    'Use bounded sparse cell pages, stable row/column slot identities and order-statistic axis maps. Apply value edits by copying only touched pages; structural inserts update axis mappings plus affected descriptors. Retain immutable snapshots for save/undo with explicit ownership and release.',
    'untouched_page_identity=A one-cell edit preserves identities of all untouched document pages.||axis_insert=Inserting a blank row before a far sparse cell does not scan or duplicate every blank row.',port='doc')
add('cdoc-m4.2','Prepare reversible cell range and axis patches','cdoc-m4.1 cfml-m4.2',
    'Prepare add/clear/fill/move/range and row/column insert/delete patches with inverse authored data. Keep formula transformation orchestration in CalcHost/CalcFml; CalcDoc stores resolved semantic patches and source-dirty regions. Preflight resource reservations before commit.',
    'patch_inverse=Apply then inverse produces byte-equivalent authored model content and stable unaffected IDs.||prepare_oom=Injected reservation failure leaves the original model and source-dirty ledger unchanged.',port='doc')
add('cdoc-m4.3','Author sheet lifecycle and relationship changes','cdoc-m4.2',
    'Create, duplicate, rename, reorder, hide and delete worksheets with correct relationships, content types, names, table/object references and active-sheet state. Preserve unrelated parts and author fresh workbook packages, not only edits to existing XML.',
    'fresh_export=Create a two-sheet workbook from no source package and reopen it in Excel without repair.||sheet_delete=Deleting a sheet removes only owned parts and leaves unrelated drawing/customXml relationships valid.',port='doc')
add('ceng-m4.1','Apply structural patches without rebuild-from-input','ceng-m3.5 cdoc-m4.2 cfml-m4.2',
    'Update coordinate-to-node maps, dependency descriptors, instantiated references and spill ownership from prepared patches. Keep stable node identities where appropriate; delete nodes explicitly. Use reference-impact indices rather than scanning unrelated formulas.',
    'delete_not_rebuild=Deleting a cell/row retains independent graph nodes and compiled plan identities while updating affected dependencies.||undo_structural=Applying an inverse structural patch restores graph semantics and recomputed values.',port='graph')
add('ceng-m4.2','Implement calculation modes and recalc command scopes','ceng-m4.1',
    'Implement automatic, automatic-except-data-tables and manual policies; ordinary F9, active-sheet Shift+F9, forced full Ctrl+Alt+F9 and dependency-rebuild Ctrl+Alt+Shift+F9. Model application-level recalc requests in CalcHost across open workbooks. Preserve cached stale values in manual mode with truthful provenance.',
    'manual_scope=An edit in manual mode marks dependencies dirty without silently calculating; F9 then computes the requested scope.||full_vs_rebuild=Force-full reevaluates all formulas without rebuilding input state; dependency rebuild is a distinct counted command.',port='engine')
add('ceng-m4.3','Apply date precision and calcPr policies','ceng-m4.2 cfml-m2.6',
    'Carry workbook 1900/1904 systems, iteration settings, compatibility version, calcPr flags and full-recalc-on-open policy in EvalEnvironment. Implement precision-as-displayed with captured persistence/mutation semantics, not merely a display-rounding shortcut; formatting edits can affect calculation under this flag.',
    'precision_display=Captured values and saved results demonstrate the selected formatting policy affects calculation as Excel does.||date_switch=Date-system changes invalidate the appropriate calculations and update serialized settings.',port='engine')
add('ceng-m4.4','Invalidate all numeric results on mode change','ceng-m4.3 cfunc-m3.23',
    'Carry NumericMode/target in all evaluation and cache keys. A mode switch invalidates formula results, dependent names, array shapes, rules and derived-object calculations, and mode-sensitive bound handles. Reuse mode-invariant syntax plans. A failed capability check changes neither workbook policy nor values.',
    'mode_full_recalc=Switching mode reevaluates every formula and labels the resulting generation correctly.||unavailable_atomic=Requesting Exact on wasm returns NumericModeUnavailable and preserves both authored revision and prior selected mode.',port='engine')
add('ceng-m4.5','Publish counted escalation facts and replay traces','ceng-m4.1',
    'Return typed RecalcFacts with evaluated IDs, invalidation causes, reschedules, SCC changes, full-recalc reasons and escalation counts. Every conservative broadening has a stable reason and replacement-owner identifier; no escalation is a silently successful ordinary edit.',
    'escalation_count=An injected conservative branch increments a named counter and appears in the receipt.||evaluation_witness=The evaluated-node set agrees with an independent CalcFml call observer and cannot include file-cached-only cells.',port='engine')
add('chost-m4.1','Prepare one atomic operation stream for structural edits','chost-m3.1 ceng-m4.1',
    'Prepare one canonical authored patch using CalcDoc and CalcFml transformations; derive the engine update from that same patch and operation identity. Commit document/engine revision changes together after preflight. Include names/rules/tables/object formula slots in reference-impact lookup.',
    'one_stream=UI and headless execution of the same structural op produce identical canonical patches and graph changes.||failure_at_each_step=Fault injection at every fallible preparation boundary leaves both states unchanged.',port='host')
add('chost-m4.2','Implement core range editing and clipboard commands','chost-m4.1 cfml-m4.1',
    'Implement clear contents/formats/all, range copy/cut/move/paste, Paste Special value/formula/format/operation/transposition modes, insert/delete cells and fill-series commands. Use one undo group per user gesture and reject partial writes into owned spill/array regions.',
    'paste_special=Formula/value/format and transpose paste produce independently checked model/results.||spill_edit_rejected=An illegal write to a spill child leaves the complete transaction unapplied.',port='host')
add('chost-m4.3','Implement sheet name table and workbook lifecycle commands','chost-m4.1 cdoc-m4.3 cdoc-m3.2',
    'Expose sheet create/copy/rename/reorder/hide/delete, defined-name create/rename/delete and structured-table lifecycle through the command registry. Maintain identity-based selections/subscriptions and repair view locators via the same patch.',
    'lifecycle_undo=Rename/reorder/delete and undo restore scoped names, tables, formulas and view anchors consistently.||duplicate_name=Invalid or conflicting sheet/table/name requests are rejected atomically with useful diagnostics.',port='host')
add('chost-m4.4','Implement sort filter and visibility commands','chost-m4.2 ceng-m3.5',
    'Implement multi-key sort, custom/list sorts, AutoFilter and advanced-filter core commands with stable data-row moves and formula/reference semantics. Separate manually hidden, filtered and outline-hidden state; update visibility dependencies and command undo receipts.',
    'sort_formula_rows=Sorting a mixed-value/formula table preserves observed Excel reference behaviour.||visibility_recalc=Filtering or hiding rows updates SUBTOTAL/AGGREGATE through visibility dependencies without a full rebuild.',port='host')
add('chost-m4.5','Implement core cell formatting and style commands','chost-m4.1 cfml-m2.6',
    'Add number/alignment/font/border/fill/protection style patches, merge/unmerge, cell styles and theme references; formatting is part of authored document state. Route numeric text/precision effects through CalcFml and the engine rather than evaluating in UI.',
    'style_undo=Applying and undoing a range style restores effective formats without copying all cell values.||precision_notification=A number-format change under precision-as-displayed triggers the required calculation effect.',port='host')
add('chost-m4.6','Implement find replace and navigation query commands','chost-m4.2',
    'Implement Find/Replace over formulas versus values, wildcard/case/whole-cell options, Go To and Go To Special selection predicates. Query sparse indexes and active scopes; replacements are one validated transaction with formula transforms as needed.',
    'replace_scope=Replacing in formulas on one sheet does not alter text values or other sheets unexpectedly.||go_to_special=Constants, formulas, blanks, errors and visible-cell queries return the correct sparse selections.',port='host')
add('chost-m4.7','Implement workbook protection and permission checks','chost-m4.3 chost-m4.5',
    'Apply sheet/workbook protection settings at the command boundary for every skin/headless caller; model locked/unlocked cells and allowed actions. Distinguish editing protection from confidentiality/encryption. Save corresponding workbook/style metadata faithfully.',
    'headless_protection=A prohibited write fails identically from grid, notebook and object-dispatch commands.||allowed_cells=An unlocked allowed cell remains editable and undo does not bypass protection policy.',port='host')
add('chost-m4.8','Expose application-level recalc and numeric preferences','chost-m4.1 ceng-m4.4',
    'Persist requested NumericMode/target as DNA metadata per workbook; record effective session capability separately. Default every app and platform to Fast; Exact is an explicit per-workbook selection presented with its availability and evidence status (plan section 5.1). Never silently downgrade a saved Exact workbook: prompt/refuse calculation until explicit user mode selection. Coordinate F9 across open workbooks.',
    'saved_mode=Opening a saved Exact workbook on wasm leaves it explicitly uncalculated/unavailable until a user selects Fast.||application_f9=The host application command reaches each eligible open workbook under its settings and collects truthful receipts.',port='host')
add('chost-m4.9','Complete bounded undo redo and save snapshot retention','chost-m4.1',
    'Use inverse patches plus bounded snapshot references, redo invalidation and transaction coalescing. Release obsolete pages/engine snapshots when history entries expire. A refused save or cancelled recalc does not append a fake undo entry; a save concurrent with later edits cannot clear their dirty status.',
    'history_release=Evicted undo entries release their unique page references under independent ownership instrumentation.||save_failure=Refused or failed save leaves current authored/engine state and undo/redo order unchanged.',port='host')
add('infra-m4.1','Prove edit recalc and save failure consistency','chost-m4.9 chost-m4.8 ceng-m4.5 infra-m1.2',
    'Retain randomized operation/reverse-operation traces covering row/column/sheet edits, names, tables, mode changes, failed recalc and refused save. Compare incremental state with replay from the same authored operation log using the same reference engine, then compare selected states to Excel.',
    'transaction_matrix=Injected recoverable failures at every preparation boundary preserve the committed revision pair.||replay_equivalence=Replayed and incrementally maintained workbooks have equal authored content, calculated values and dependency facts.',port='oracle',platform='oracle')

# M5: hard calculation cases are real first-version implementation work.
add('infra-m5.1','Capture iterative and data-table scheduling witnesses','infra-m1.2',
    'Capture small SCCs with different cell/name order, seeds, damping, divergence, error/text outputs and mode settings; capture 1D/2D TABLE cases, cyclic tables, volatile inputs and automatic-except-tables. Retain the actual Excel iteration/results observations and ambiguity notes, not hard-coded production response tables.',
    'iteration_witnesses=Each case records formulas, settings, initial state, observed outputs and target fingerprint.||table_witnesses=One- and two-variable tables plus cyclic/volatile cases have independently captured expected values and no fabricated trace claims.',port='oracle',platform='oracle')
add('ceng-m5.1','Detect and maintain workbook strongly connected components','ceng-m4.1',
    'Implement SCC classification over cell, contextual-name and dynamic-reference participants. Update affected graph components after edits and dynamic edge changes; use a deterministic component/order policy. Non-iterative cycles return the observed circular-reference/error policy.',
    'name_cell_scc=A cycle spanning two sheets and a defined name is one SCC.||dynamic_scc_change=Changing an INDIRECT target can create or break a cycle and classification changes before publication.',port='graph')
add('ceng-m5.2','Implement captured iterative scheduling and convergence','ceng-m5.1 infra-m5.1',
    'Implement the chosen target’s observed iteration initialization, node ordering/update visibility, maximum-iteration and maximum-change policy. Use CalcFml for every iteration. Handle mixed numeric/error/text values and early cancellation explicitly; record iterations and termination reason.',
    'iterative_excel=All committed SCC witness cases reproduce captured outputs and termination classifications.||not_literal_lookup=Perturb an input outside the capture list and verify recurrence steps through an independent evaluation observer rather than a table lookup.',port='engine')
add('ceng-m5.3','Handle iterative arrays dynamic topology and failure publication','ceng-m5.2 ceng-m3.4',
    'Specify and implement shape/dynamic-edge changes during iterative calculation using retained witnesses. Reclassify components when needed, count reschedules and keep publication atomic. Resource cancellation/divergence never substitutes a file-cached final answer.',
    'iteration_shape=An iterative formula that changes a spill/dynamic reference follows the captured behaviour or a counted explicit unsupported result until its witness is implemented.||iteration_cancel=Cancellation leaves the previous published generation stale and authored changes intact.',port='engine')
add('cdoc-m5.1','Model and author what-if data-table formulas','cdoc-m3.3',
    'Read/write TABLE formula attributes, input-cell references, row/column orientation and table result blocks. Preserve the distinction from structured ListObject tables. Expose editable descriptors and relationships for host operations.',
    'table_encoding=One- and two-variable TABLE definitions round-trip with the same inputs and output region.||table_authoring=A newly authored data table reopens without repair and Excel calculates the expected grid.',port='doc')
add('ceng-m5.4','Evaluate one-variable data tables with input overlays','ceng-m4.2 cdoc-m5.1 infra-m5.1',
    'Implement one-variable data-table execution as temporary input overrides and nested work items on the same graph/evaluator. Restore ordinary workbook inputs after each run, union required read facts and publish the complete output block as one calculation generation.',
    'one_variable_table=Column- and row-oriented tables match independent Excel captures.||override_isolation=Temporary input values are never committed to authored cells or seen by a later unrelated calculation.',port='engine')
add('ceng-m5.5','Evaluate two-variable data tables and cyclic table semantics','ceng-m5.4 ceng-m5.2',
    'Extend the same execution path to two-variable combinations, ordering, table self-dependency and captured volatile/iterative interactions. Honor automatic-except-tables and explicit F9 triggers without recreating the document per scenario.',
    'two_variable_table=Every output of the captured 2D table is computed from actual scenario evaluation.||table_cycle=The witnessed table self-dependency is handled using table semantics rather than rejected as an ordinary graph cycle.',port='engine')
add('chost-m5.1','Expose iterative settings and what-if data-table commands','chost-m4.8 ceng-m5.3 ceng-m5.5',
    'Implement iteration settings dialogs/commands and TABLE create/edit/remove commands with reversible authored patches. Show nonconvergence/cancellation and table-calculation state; never report refresh success before values are actually calculated.',
    'table_command=Creating a table through the command model changes the actual workbook and calculates its outputs.||iterative_receipt=Changing iteration limits recalculates affected SCCs and exposes the real termination facts.',port='host')
add('cdoc-m5.2','Model conditional-format rules and formula anchors','cdoc-m3.4',
    'Read/write rule priorities, applies-to multi-areas, relative anchors, differential styles, stop-if-true and supported extension rule payloads. Author add/edit/remove/reorder operations while preserving unknown rule extensions unless a conflicting edit must refuse.',
    'cf_roundtrip=Formula and built-in conditional rules retain priorities, ranges and differential styles through edits.||relative_anchor=Changing applies-to regions preserves the intended formula anchor semantics.',port='doc')
add('cdoc-m5.3','Model data-validation rules and input messages','cdoc-m3.4',
    'Read/write formula/list/whole/decimal/date/time/text-length/custom validation descriptors, applies-to ranges, blank policy, error styles and input messages. Preserve list separators/locale file syntax and extension payloads.',
    'dv_roundtrip=Inline-list and formula-based validation survive save/open with their messages and error styles.||validation_authoring=New and deleted validation regions produce valid Excel XML and no orphan descriptors.',port='doc')
add('cfml-m5.1','Compile per-cell rule contexts for CF and DV','cfml-m4.2 cdoc-m5.2 cdoc-m5.3',
    'Compile shared rule templates with applicability anchor and target-cell contexts. Reuse the same coercion, reference and name semantics as ordinary formulas; expose rule read footprints without generating a formula object for every blank cell in a whole-column applies-to range.',
    'rule_relative=Applying the same rule to two cells instantiates different correct relative reads.||rule_sharing=A whole-column rule shares one template and does not materialize one million plans.',port='fml')
add('ceng-m5.6','Register conditional-formatting dependency participants','ceng-m3.1 cfml-m5.1',
    'Represent rule-template applicability and evaluated target-cell instances in the graph. Track conditional numeric/text/date/formula predicates and view-demand evaluations, including rule priority/stop-if-true. Invalidation is compressed over applicable regions.',
    'cf_recalc=Changing a referenced cell updates the affected rule result delivered to the skin.||no_dense_rule_grid=A whole-column rule stores compressed applicability plus requested instances, not one node per blank cell.',port='graph')
add('ceng-m5.7','Evaluate color scales data bars and icon sets','ceng-m5.6',
    'Add aggregate rule participants for range-dependent thresholds/percentiles, explicit threshold formulas and per-cell style classifications. Keep conditional formatting separate from authored base styles and prohibit circular display-precision feedback unless captured semantics define it.',
    'scale_thresholds=Changing a range extremum updates the dependent color/bar/icon facts according to captured Excel thresholds.||priority_stack=Multiple conflicting rules obey priority and stop-if-true without mutating base styles.',port='engine')
add('ceng-m5.8','Evaluate validation candidates in staged input contexts','ceng-m3.3 cfml-m5.1',
    'Evaluate DV against a proposed transaction snapshot and the appropriate old/new value context from oracle witnesses. Register dependencies for rule indicators; treat validation prompting as host policy, not automatic rejection of all programmatic writes. Support allow-blank and stop/warning/information outcomes.',
    'validation_candidate=A candidate referring to another edited cell in the same batch sees the correct staged value.||programmatic_policy=Typed validation results distinguish a UI stop rejection from an explicitly permitted programmatic write path.',port='engine')
add('chost-m5.2','Implement CF rule authoring and projection commands','chost-m4.5 ceng-m5.7',
    'Implement rule create/edit/delete/reorder/apply-to commands and style overlay subscriptions. Reuse ordinary formula editing assistance for rule formulas. Expose color-scale/bar/icon facts to every skin and persist only authored rules, not accidental view caches.',
    'cf_command_roundtrip=A rule authored through the command model affects visible cells and survives Excel reopen.||cf_undo=Undo restores priorities/applicability and the calculated overlay deltas.',port='host')
add('chost-m5.3','Implement validation authoring prompts and rejected edits','chost-m4.2 ceng-m5.8',
    'Implement DV editor commands, dropdown list projection, input messages, invalid-data indicators and context-aware prompt policies for typed entry versus paste/script paths. A rejected edit leaves both document and engine unchanged; accepted warnings record the actual authored write.',
    'stop_is_atomic=A stopped invalid entry changes neither workbook state nor undo history.||warning_override=Accepting a warning commits exactly once, persists the rule and publishes the correct dependent values.',port='host')
add('infra-m5.2','Capture CF DV and precision-as-displayed witnesses','infra-m1.2',
    'Create independent Excel captures for relative rule anchors, priorities, aggregate thresholds, DV edit/paste behaviours, date systems and precision-as-displayed changes/saves. Include cross-sheet defined names and full-column applicability.',
    'rule_captures=Expected rules, visible classifications and rejected/accepted edit outcomes come from real Excel observations.||precision_capture=The pre/post save values and formatting flags are retained so display rounding cannot masquerade as calculation semantics.',port='oracle',platform='oracle')
add('infra-m5.3','Prove hard-calculation first-version coverage','chost-m5.1 chost-m5.2 chost-m5.3 infra-m5.2',
    'Run the product HostState path across the SCC, TABLE, CF and DV capture suites on native and wasm. Count all required participant instances independently. Require calculation/evaluation, not preservation or cached outcomes, for these mandatory features.',
    'hard_features_calculate=All required fixture participants execute the real evaluator and match their recorded outputs/errors/shapes.||no_typed_unsupported_exit=A mandatory SCC/TABLE/CF/DV fixture marked unsupported prevents this milestone from passing.',port='oracle',platform='oracle')
# Additional shared model and open-workbook integration, kept inside the same graph architecture.
add('cdoc-m3.6','Model styles themes rows columns and panes','cdoc-m1.3',
    'Add interned styles/number formats/theme references, row/column dimensions and visibility, merges, outlines and view/pane state as sparse document metadata. Preserve style IDs on no-op save and avoid per-cell copies for shared styles.',
    'style_identity=An untouched style/shared-string table retains stable IDs and byte-preserved parts.||sparse_dimensions=Setting one whole-column width does not create individual cell records.',port='doc')
add('ceng-m3.9','Unify connected open workbooks in one calculation domain','ceng-m3.7',
    'Allow one EngineState calculation domain to index connected open workbooks by WorkbookId, using a revision vector and the same graph/scheduler. External closed snapshots remain provider leaves. Do not introduce inter-sheet or inter-workbook fixed-round federation.',
    'open_book_chain=A dependent chain spanning two open workbooks is topologically scheduled in the same graph.||book_identity=Two workbooks with identically named sheets and cells cannot alias nodes, names or cache entries.',port='engine')
add('chost-m4.10','Coordinate multiple open documents and cross-book transactions','chost-m4.8 ceng-m3.9',
    'Maintain a Host-owned DocumentSet plus one calculation domain for connected open books. Route commands with workbook IDs, prepare all affected document revisions before atomic cross-book commit, and publish per-workbook deltas with the shared revision vector. Save remains per-document unless an explicit multi-save is requested.',
    'cross_book_atomic=A multi-workbook edit either commits all requested authored patches and graph effects or none.||one_book_save=Saving one book does not mark another dirty book saved or discard shared dependency state.',port='host')
add('ceng-m5.9','Calculate cycles crossing linked open workbooks','ceng-m5.2 ceng-m3.9',
    'Apply the same SCC/iteration execution to open-workbook links with captured application-level settings and initial state. Capture unresolved/conflicting setting cases and report explicit policy; do not solve them with cross-book rounds or stale link caches.',
    'cross_book_cycle=A retained two-book iterative witness calculates through one SCC and matches Excel observations.||closed_link_transition=Closing a linked workbook changes it to a versioned provider leaf with truthful provenance and no hidden iterative result reuse.',port='engine')

# Phase-2 attachment points with real phase-1 callers; no VBA/XLL implementation.
add('cfunc-m4.1','Expose a source-neutral mutable UDF registry','cfunc-m3.22 cfml-m2.3',
    'Implement caller-owned RegistryState with register/unregister/invalidate generations, source IDs, caller context and volatility/thread-safety/async declarations. Built-in callable/OneCalc user-function registration uses it now; future sources are opaque IDs, not dead VBA/XLL enum variants.',
    'registry_mutation=Registering/replacing/unregistering a callable changes resolution and invalidates retained call-site handles.||source_neutral=Two providers can register distinct source IDs without any platform-specific dependency.',port='udf')
add('cfml-m4.3','Bind UDF generations and asynchronous result tokens','cfml-m3.3 cfunc-m4.1',
    'Compile registered function calls to generation-checked handles. Represent pending async results/cancellation tokens explicitly and report function-registry and caller dependencies. A production OneCalc callable registry and replay provider exercise the route without a VBA/XLL runtime.',
    'stale_handle=A plan compiled before unregister/re-register cannot call a stale function implementation.||pending_result=A pending async replay result is distinguishable from a worksheet error or a calculated scalar.',port='fml')
add('ceng-m4.6','Apply external RTD and async completion operations','ceng-m3.9 cfml-m4.3',
    'Accept revisioned ExternalValueChanged, RtdChanged and AsyncCompleted operations with source/subscription IDs. Reject stale completions and invalidate subscribed nodes separately from volatile recalc. Use the production capture/replay adapter as the phase-1 provider.',
    'external_vs_volatile=An RTD update reevaluates its subscribers without simulating a volatile full tick.||stale_completion=A cancelled or superseded async result cannot overwrite the current result generation.',port='engine')
add('chost-m4.11','Add events and exclusive batch execution mode','chost-m4.10 ceng-m4.6',
    'Publish ordered open/change/calculate/save events after the relevant commits. Provide an exclusive serialized BatchExecute transaction mode with queued external updates and bounded event reentrancy. The production headless batch command uses it now; future macros use the same facility.',
    'event_order=Change precedes Calculate for an edit/recalc and observers see committed state only.||exclusive_batch=Concurrent external updates are queued and applied in order after the exclusive batch, with no lost update.',port='vba')
add('chost-m4.12','Expose object dispatch over host commands','chost-m4.11',
    'Define Application/Workbook/Worksheet/Range object handles with generations and late-bound member dispatch translated into canonical commands/queries. A production calcctl object command and formula range inspector call it. Keep COM types and IDispatch outside the portable host.',
    'object_write=Writing Range.Value through headless object dispatch produces the same operation/undo receipt as grid entry.||stale_object=A handle to a deleted sheet/range fails with a typed stale-object error.',port='vba')
add('chost-m4.13','Reserve add-in callback capabilities IDE panels and range picking','chost-m4.12',
    'Expose typed host-service queries/commands suitable for later Excel12v adaptation; add capability discovery, range-selection sessions and dock/panel registration. The production formula/range picker and diagnostics panel consume these now. Leave XLOPER12 marshalling, COM, VBA, forms and native loading outside phase 1.',
    'range_picker=A formula dialog requests a range, receives selection/cancel through the shared host service and creates no separate workbook model.||capability_discovery=The shipped shell reports unavailable VBA/XLL capabilities honestly while using the real generic panel/service route.',port='vba')

# M6 CalcDoc: finish the core .xlsx/.xlsm authored model and fidelity obligations.
DOC_FINISH = [
('Author complete cell formats and style tables','cdoc-m3.6', 'Implement number/alignment/font/border/fill/protection records, differential formats, theme/tint references, merged-cell styles and style deduplication without renumbering untouched source IDs.', 'all_style_tabs=Each Format Cells category survives independent XML inspection and Excel reopen.||style_dedup=Equivalent newly authored styles share records while untouched original indices remain stable.'),
('Author row column outline and pane metadata','cdoc-m3.6', 'Write heights/widths, best-fit/custom flags, hidden rows/columns, outlines, freeze/split panes and sheet view zoom/selection with sparse default intervals.', 'pane_roundtrip=Freeze and split panes plus hidden/outlined axes reopen in Excel with their authored state.||default_intervals=Setting a million-row default does not allocate a million cell or style records.'),
('Model comments notes and hyperlinks','cdoc-m3.5', 'Read/edit/write legacy notes, threaded comments/person relationships where supported, hyperlink targets/tooltips and drawing anchors. Preserve unedited unsupported comment extensions with a typed edit refusal when necessary.', 'comments_roundtrip=Authored notes/comments and hyperlinks reopen without lost ownership or invalid relationships.||link_safety=External hyperlinks are preserved as data and do not trigger network access on open.'),
('Preserve macro projects signatures and package extensions','cdoc-m3.4', 'Preserve vbaProject.bin, customXml, content types and package/application extension parts on safe edits. Detect signatures invalidated by edits and require an explicit refusal or acknowledged signature-removal action; never claim a modified signed package remains signed.', 'macro_preserved=A value/formula edit leaves untouched vbaProject.bin hash-identical and never executes it.||signature_policy=An edit invalidating a signature cannot silently retain a misleading valid-signature status.'),
('Write calcPr calcChain and trustworthy formula caches','cdoc-m3.3 ceng-m4.3', 'Serialize all calculation settings and cache provenance policy. Drop or regenerate stale calcChain only under the documented lawful policy; set recalc-on-open flags for stale/uncalculated results. Keep raw FileCached bytes when fidelity requires preservation, but never relabel them as newly calculated.', 'cache_provenance=Saved caches correspond only to the recorded calculation generation or are explicitly stale with recalc policy.||calcchain_edit=Structural edits do not leave a dangling calculation-chain cell or sheet reference.'),
('Complete source-preserving package authoring and Save As','cdoc-m4.3', 'Audit create/update/delete of all core part/relationship/content-type families, including empty/fresh workbooks and Save As. Stream untouched payloads and serialize changed regions with a fidelity ledger of copied, rewritten, normalized and refused parts.', 'fresh_core_workbook=A source-free workbook with names, tables, styles, rules and formulas opens in Excel without repair.||ledger_matches_bytes=An independent ZIP diff agrees with every fidelity-ledger copied/rewritten claim.'),
('Enforce transactional save output and lazy source access','cdoc-m6.6', 'Prepare immutable save snapshots, write to temporary native output or a completed browser download/OPFS object, and acknowledge success only after output completion. Bound decoded XML retention and do not hold a second full authored cell table during save.', 'failed_write=An injected writer failure leaves the previous native file intact and never returns a successful browser-save receipt.||memory_accounting=An allocation/ownership report for a 100MB fixture separates source bytes, parsed input, results and transient save buffers without duplicate authored cell stores.'),
]
for i,(title,deps,do,checks) in enumerate(DOC_FINISH,1):
    add(f'cdoc-m6.{i}',title,deps,do,checks,port='doc')

# M6 CalcFml format completeness / language service edge cases.
add('cfml-m6.1','Complete number-format sections locales and numeric parsing','cfml-m2.6 cfunc-m3.25',
    'Complete conditional/multi-section, scaling, fraction/scientific, elapsed-time, text, escape/padding and locale-dependent formats. Share the rounding/number-to-text policy with TEXT and entry parsing; preserve invariant serialization. Add compatibility-version and long-text boundaries.',
    'format_matrix=All declared format grammar families have independent Excel-rendered string witnesses.||format_value_seam=TEXT and displayed cells agree on the same format/locale/date-system cases without two numeric-to-text implementations.',port='format')
add('cfml-m6.2','Complete editor reference modes and diagnostics','cfml-m2.5 cfml-m4.2',
    'Implement point-mode range insertion, F4 cycling, reference colorization, name/table completion and locale separators using the same lossless source tree. Preserve IME composition and source-version cancellation at the app adapter boundary.',
    'reference_cycle=Repeated F4 cycles relative/mixed/absolute references without modifying quoted strings or unrelated spans.||incomplete_formula=Diagnostics and completion remain stable for incomplete nested calls and non-ASCII names.',port='parse')

# M6 host command families still needed for full core UI.
add('chost-m6.1','Implement panes dimensions grouping and sheet-view commands','chost-m4.5 cdoc-m6.2',
    'Implement freeze/split panes, row/column size and auto-fit, hide/unhide, outline/group/ungroup and zoom/view state through canonical commands. Auto-fit uses the shared text measurement service; view-only changes must not dirty unrelated formula nodes.',
    'view_commands=Headless commands and grid gestures produce equal authored/view patches.||no_recalc_on_zoom=Zoom and pane movement issue no CalcFml evaluation calls.',port='host')
add('chost-m6.2','Implement comments notes hyperlinks and review commands','chost-m4.3 cdoc-m6.3',
    'Implement comments/notes lifecycle, hyperlink editing/follow policy and review/protection-related queries in one command family. Keep network navigation permission in platform adapters and expose editable-object projections to all skins.',
    'comment_commands=Create/edit/delete/undo a note or comment through HostState and reopen its saved package.||hyperlink_policy=A workbook cannot cause automatic external navigation on load or recalc.',port='host')
add('chost-m6.3','Implement formula audit trace and watch commands','chost-m4.12 ceng-m4.5',
    'Expose Evaluate Formula stepping, precedent/dependent queries, watch subscriptions, error-checking facts and selection aggregates. Use CalcFml trace policy plus engine read facts; do not build a second formula interpreter for the audit UI.',
    'evaluate_formula=Stepping a formula yields the same result and branch/provider calls as normal evaluation.||watch_delta=A watch updates from value deltas rather than rereading its sheet or returning a stale unlabelled cache.',port='host')
add('chost-m6.4','Implement document backstage and recovery commands','chost-m4.9 cdoc-m6.7',
    'Implement new/open/close/save/save-as/recent/export, unsaved-change decisions and document recovery metadata in the application controller. Keep credentials/file dialogs in adapters; macro/connection trust remains explicit and disabled by default for untrusted sources.',
    'unsaved_close=Cancel/discard/save responses have distinct observable outcomes with no silent data loss.||recovery_replay=A retained authored operation journal recovers the last committed state without claiming a crashed partial save succeeded.',port='host')

# Excel UI families: each bead implements UI only and consumes an already callable headless command.
UI = [
('Build ribbon backstage quick access and key tips','dnacalc-m2.5 infra-m3.1 chost-m6.4', 'Render the complete captured ribbon/backstage/quick-access inventory from generated command metadata. Implement Alt key-tip state, contextual tab activation and customization persistence; use original or permissively licensed icons and retain asset provenance.', 'ribbon_commands=Activating ribbon, quick-access and backstage entries invokes the correct canonical command.||keytips=Alt sequences navigate and execute without stealing editor text input.'),
('Implement range multi-area and axis selection','dnacalc-m2.5', 'Implement mouse/keyboard range selection, Ctrl/Shift extension, multi-area sets, entire row/column/sheet selection and selection anchors over virtualized geometry. Selection is view state; commands carry resolved areas.', 'selection_matrix=Captured selection gestures produce expected selected areas and active cells.||sparse_selection=Selecting an entire worksheet does not enumerate its cells.'),
('Implement edit enter point and IME modes','dnacalc-m6.2 cfml-m6.2', 'Implement F2 edit versus enter mode, point-mode reference picking, F4 cycling, multi-line formula bar, input focus/commit/cancel and IME handling. Keep dirty editor-buffer undo separate from workbook undo.', 'mode_matrix=Keyboard and pointer sequences match expected commits, cursor movement and reference insertion.||ime_text=Composition events commit once and do not issue premature workbook edits.'),
('Implement fill handle and AutoFill series','dnacalc-m6.2 chost-m4.2', 'Add fill grip gestures, auto-fill options, copy versus series selection, dates/numbers/custom lists and repeated-formula preview. Execute one host fill command with correct reference adjustment and one undo group.', 'autofill_series=Numeric/date and mixed-reference fill cases match independently captured workbook results.||fill_undo=Undo removes the complete fill and restores prior cells in one step.'),
('Implement drag move copy and insert gestures','dnacalc-m6.2 chost-m4.2', 'Implement range drag with copy modifiers, move/insert destinations, conflict prompts and ghost previews. Use host preflight for merged/spill/protected targets; do not mutate the grid optimistically as a second truth.', 'drag_reference=Moving a formula range updates dependent references using the host transform.||drag_cancel=Cancelling a conflict prompt changes no authored cell or undo history.'),
('Implement clipboard and Paste Special UI','dnacalc-m6.3 chost-m4.2', 'Support text/TSV/HTML clipboard interchange, an internal typed payload and publicly documented native Excel formats through platform adapters. Implement Paste Special dialogs, reference adjustment and Excel interop tests; unsupported browser-native formats are visible capabilities, not fake successes.', 'excel_clipboard=Copy/paste to and from real Excel preserves declared values/formulas/formats for each supported format.||paste_special_ui=Dialog options reach the same tested headless commands and retain one undo group.'),
('Implement freeze split and axis layout controls','dnacalc-m6.2 chost-m6.1', 'Expose freeze/split panes, row heights, column widths/auto-fit, hide/unhide and grouping/outline controls. Use logical segmented scroll positions to reach the full grid without relying on one browser element of enormous height.', 'panes_and_scroll=Selections and edits remain on the correct cells across panes and at far-sheet positions.||outline_visibility=Grouping/hiding updates view geometry and visibility-dependent formula outputs.'),
('Implement Format Cells Number and Alignment','dnacalc-m6.3 chost-m4.5 cfml-m6.1', 'Implement Number/custom formats, locale previews, Alignment/orientation/wrap/shrink/merge controls using the shared format and text services. Preserve mixed-selection states and preview without committing.', 'format_dialog=OK changes the actual styles and shared formatter output; Cancel changes neither.||mixed_selection=Mixed number/alignment values are represented explicitly rather than silently overwritten on dialog open.'),
('Implement Format Cells Font Border Fill and Protection','dnacalc-m6.8 chost-m4.7 cdoc-m6.1', 'Complete Font/Border/Fill/Protection tabs with previews, themes/tints, edge-specific borders, locking and hidden formulas. Apply one style patch and integrate accessible color controls.', 'remaining_format_tabs=Each tab writes independently verified style records and round-trips through Excel.||protection_effect=Lock/hidden-formula settings alter command visibility/access under sheet protection as expected.'),
('Implement cell styles themes and format painter','dnacalc-m6.9', 'Expose styles gallery, theme changes, clear-formatting and format-painter interactions through shared style commands. Theme tokens remain distinct from resolved colors; selection previews do not mutate workbook styles.', 'theme_references=Changing a theme updates theme-bound cells but not explicitly colored cells.||format_painter=Copying a style changes only formatting, not formulas or values.'),
('Implement Name Manager and name-box navigation','dnacalc-m6.3 chost-m4.3 cfml-m3.1', 'Implement scoped name list/create/edit/delete, refersto formula assistance, filters and error display; distinguish internal locator names from user names. Name-box selection uses the same typed reference service.', 'name_manager=Editing a scoped name updates real dependent formulas and survives save/reopen.||internal_locators=Normal Name Manager use cannot accidentally rename hidden DNA locator names without an explicit advanced action.'),
('Implement conditional-formatting UI and rule manager','dnacalc-m6.9 chost-m5.2', 'Implement rule galleries, formula editing, priorities/stop-if-true, applies-to picking and rules manager. Render color scales/data bars/icon sets from engine facts over the actual grid and accessible projections.', 'cf_manager=Reordering rules updates the correct visible style and survives Excel reopen.||cf_not_fake=Changing an input used by a rule updates pixels and accessible style facts through a real engine delta.'),
('Implement data-validation dialog dropdowns and prompts','dnacalc-m6.3 chost-m5.3', 'Implement the validation settings/input/error tabs, source range picking, dropdowns, messages and invalid-entry prompts. Match edit/paste context policy and allow explicit warning override.', 'dv_stop=An invalid stopped entry retains the editor state and leaves the workbook unchanged.||dv_list=A formula-backed dropdown refreshes after its source changes and selecting an item issues a normal edit.'),
('Implement Find Replace Go To and Go To Special','dnacalc-m6.3 chost-m4.6', 'Expose find/replace options/results and Go To/Special dialogs with keyboard traversal, sparse result navigation and preview of replace-all. Invoke canonical query/edit commands with a single replace-all transaction.', 'find_replace_ui=Formula/value scope and wildcard/case options reach the correct results and actual replacements.||goto_special_ui=Visible-only/error/formula selections are represented correctly on the grid.'),
('Implement sort AutoFilter and advanced-filter UI','dnacalc-m6.2 chost-m4.4', 'Implement multi-key/custom sort dialogs, header filter menus, search/checklist/numeric/date/text filters and advanced-filter dialogs. Preserve selection and table context; changes use host row/visibility operations.', 'filter_ui=Filtering affects actual row visibility and SUBTOTAL results rather than hiding pixels alone.||sort_dialog=Multi-key sort and undo produce the same workbook as the headless command tests.'),
('Implement table creation and contextual table tools','dnacalc-m6.15 chost-m4.3', 'Implement create/resize/rename/convert table, header/totals row options, calculated columns and table styles; show the contextual Table ribbon and structured-reference editing assistance.', 'table_tools=A table created and edited from UI has real table parts and working structured formulas.||table_convert=Converting to a range preserves cell contents and adjusts dependent references correctly.'),
('Implement insert delete cells rows columns and sheets UI','dnacalc-m6.2 chost-m4.3', 'Expose context menus/dialogs and shortcuts for shifted-cell/axis/sheet insert/delete, sheet duplicate/move/hide/unhide and protection-aware prompts. Use the same preflight and undo commands as headless operations.', 'insert_delete_ui=Reference transforms and persisted sheet state match the host command tests.||protected_lifecycle=Prohibited structural operations are blocked equally from menu and keyboard.'),
('Implement calculation options and what-if table dialogs','dnacalc-m6.3 chost-m5.1 chost-m4.8', 'Expose automatic/manual/except-tables, F9 variants, iteration options, date/precision settings and Exact/Fast target/capability status. Add the actual Data Table dialog; reserve Goal Seek/Solver/other analytic commands for their area plan.', 'calculation_options=Mode/settings changes reach real engine policy and visibly update provenance.||table_dialog=The dialog creates a real calculated data table rather than a static pasted grid.'),
('Implement Evaluate Formula trace watch and error tools','dnacalc-m6.3 chost-m6.3', 'Build Evaluate Formula stepping, Trace Precedents/Dependents arrows, Watch Window and Error Checking UI. Use shared trace/read facts; navigate multi-sheet/range precedents and label stale values.', 'formula_audit_ui=Stepping and watch values equal normal production evaluation for the selected cell.||trace_navigation=Following a cross-sheet precedent opens the correct sheet/cell without synthesizing a separate expression evaluator.'),
('Implement comments notes hyperlinks and protection UI','dnacalc-m6.3 chost-m6.2 chost-m4.7', 'Expose comments/notes editing, hyperlinks, protection dialogs and allowed-action lists with correct keyboard/focus behaviour. Display threaded/unsupported-preserved objects honestly rather than flattening ownership silently.', 'review_ui=Notes/comments/protection changes persist and apply to commands in all skins.||hyperlink_prompt=External navigation requires the platform trust action and does not occur on hover or load.'),
('Implement status aggregates zoom sheet tabs and window navigation','dnacalc-m6.2 chost-m6.1 chost-m6.3', 'Complete sheet-tab switching/reordering, status-bar aggregate selection, zoom controls, split/window navigation and accessibility summaries. Aggregate selections through host/provider services with sparse semantics, not screen-text parsing.', 'status_aggregates=Displayed sum/count/average agree with typed selected values including blanks/errors.||tabs_views=Switching sheets retains the appropriate per-sheet view/selection state without altering workbook formulas.'),
('Complete keyboard routing accessibility and command parity','dnacalc-m6.1 dnacalc-m6.21 chost-m4.13', 'Reconcile all core shortcut/keytip/dialog commands with the captured inventory; implement a bounded accessible DOM mirror for the canvas grid, focus order and screen-reader announcements. Reserve Developer/IDE/forms/add-in slots with explicit phase-2 status and use the real panel/range-picker capability.', 'keyboard_parity=Every core inventory shortcut executes its named command in the correct editor/selection context.||screen_reader=An actual accessibility-tree test can navigate/read/edit a cell and identify errors/spills without reading canvas pixels.'),
]
for i,(title,deps,do,checks) in enumerate(UI,1):
    add(f'dnacalc-m6.{i}',title,deps,do,checks,port='gridui',priority=1)

# Keep non-Excel skins feature-complete as the workbook feature set grows.
add('dnacalc-m6.23','Complete non-Excel skin rules tables and protected editing','dnacalc-m2.6 chost-m5.2 chost-m5.3 chost-m4.7',
    'Expose validation/conditional-format effects, table membership, protected editing, structural changes and iterative/table-calculation diagnostics through the shared detail/command capabilities in notebook, tree and canvas. Preserve their distinct interactions without implementing a private calculation path.',
    'all_skin_features=The same rule/protection/table workbook exhibits the same values and accepted/rejected edits in all four skins.||skin_switch_no_loss=Switching and saving from each skin preserves the others’ layout/prose and canonical workbook data.',port='skins',priority=0)
add('dnacalc-m6.24','Test original visual assets browser limits and input interoperability','dnacalc-m6.22 dnacalc-m6.23',
    'Record license/provenance for icons/fonts/illustrations, add pixel-level rendering checks alongside geometry/receipt tests, and test browser clipboard/key reservation/printing capability boundaries. Provide explicit alternate invocation routes when a browser intercepts a native shortcut; do not claim those limitations are exact native parity.',
    'real_rendering=Screenshot/visual tests catch missing canvas text/overlays even when debug counters are correct.||platform_capabilities=Unsupported native clipboard/key operations are exposed with tested alternatives and correctly marked parity status.',port='gridui')
add('onecalc-m6.1','Complete OneCalc function exploration and fixture persistence','onecalc-m2.1 cfunc-m3.25 cfml-m6.2',
    'Generate searchable function/operator help, arguments/examples, numeric mode/evidence status, trace and reference-provider fixtures from the single catalogue. Persist OneCalc session data independently of CalcHost/workbook operations; support import/export of formula test cases, not a hidden workbook engine.',
    'catalogue_explorer=Every migrated public function has generated help and an invocable fixture surface.||onecalc_boundary=Native and browser dependency closures still exclude CalcEngine and CalcHost, including UI helper crates.',port='func')
add('infra-m6.1','Prove complete core UI and document authoring','dnacalc-m6.24 onecalc-m6.1 cdoc-m6.7 infra-m3.1 infra-m1.2',
    'Execute every core command-family acceptance on the shipped app path, then save/reopen representative workbooks in Excel. Produce a parity ledger with test links; separable/phase-2 commands remain visibly pending and are not counted as core completion. Verify read/edit/write fidelity independently of the writer’s own ledger.',
    'core_command_coverage=Every core inventory row maps to an executed interaction plus headless command test; zero missing rows.||document_matrix=Fresh and edited xlsx/xlsm core fixtures reopen in Excel without repair and untouched-part hashes match policy.',port='oracle',platform='oracle')

# M7: retained measurements and conformance, not performance targets.
add('infra-m7.1','Define licensed corpus manifests and stratified coverage','infra-m0.6 infra-m1.1',
    'Implement a content-addressed corpus manifest with source/license/redistribution disposition, acquisition hashes, Excel target, formula/participant counts and feature strata. Start with owner-authored or newly generated permissively licensed fixtures. Treat Enron, EUSES and FUSE as acquisition candidates, not automatically permissive collections; quarantine uncleared documents.',
    'license_gate=A manifest entry without a recorded redistribution basis cannot enter the published corpus.||strata_counts=Hidden sheets, names, arrays, rules, external links and multi-sheet cases are independently enumerated rather than counted from engine status flags.',port='oracle',platform='tooling')
add('infra-m7.2','Generate and capture cross-feature workbook families','infra-m7.1 infra-m1.2 infra-m5.3',
    'Build parameterized Excel-authored workbook generators for scope shadowing, mixed refs, sheet edits, arrays/spills, names/LAMBDAs, dates/locales, iteration, TABLE, CF and DV. Retain at least 1,000 distinct workbooks, including 200 multi-sheet and 50 containing three or more interacting feature categories. These are evidence-volume floors, not runtime targets; report generator independence and minimize failures.',
    'corpus_floor=Independent manifest inspection proves the stated workbook/stratum counts without counting byte-identical copies.||reproducible_capture=A sampled case regenerates its authored inputs and records fresh Excel results through the one capture service.',port='oracle',platform='oracle')
add('infra-m7.3','Acquire public corpus candidates with provenance review','infra-m7.1',
    'Inventory the original Enron/EUSES/FUSE distribution sources, record collection and individual-file rights, privacy and security constraints, and acquire only permitted fixtures. Maintain a private quarantined evaluation lane where permitted but not redistributable; do not bundle such workbooks. Emit a retained admission report even when no external collection qualifies.',
    'admission_report=Every admitted file has its original source, content hash and explicit rights disposition; rejected candidates remain visible in the report.||privacy_safety=Macros, credentials, external connections and potentially sensitive content are not executed or published by corpus ingestion.',port='fresh',platform='tooling',scope='Do not claim that public availability is a permissive license. Corpus access does not authorize redistribution or macro execution.')
add('infra-m7.4','Unify live and cached verdicts with raw numeric observations','infra-m7.2 infra-m1.1',
    'Extend the single verdict path with raw Value2 binary64 observations, exact error/type/array shape, saved XML cache text and freshness/Excel-build evidence. Keep cached-XML-only matches distinct from bit-certified raw observations; inspect formula inventories independently. Emit calculated/formula counts first, then matched/calculated and unsupported/stale/failure counts.',
    'cached_not_pass=A formula whose input cache matches Excel but which was never evaluated contributes zero calculated/matched cells.||raw_bits=Two adjacent binary64 values that serialize ambiguously in an XML cache are distinguished by retained raw observations.||population_reconciles=Every formula coordinate has exactly one provenance/verdict category, including errors, spills and not-modeled cells.',port='oracle',platform='oracle')
add('infra-m7.5','Port one black-box smart-fuzzer campaign runner','infra-m7.4 infra-m0.5 cfunc-m3.25',
    'Retain one search/capture/replay pipeline for generated function inputs and calculation-graph candidates, using only public documentation and black-box Excel observations. Minimize counterexamples; retain all mismatches, crashes and target fingerprints; never train a literal lookup table into the shipping evaluator. Emit a campaign manifest that permits interrupted runs to resume without losing witnesses.',
    'failure_retention=An injected mismatch is minimized, saved and reproduced by the same verdict path on all portable targets.||no_answer_table=Candidate promotion requires a general implementation plus held-out witnesses; memorized expected-value branches fail the production-source gate.',port='oracle',platform='oracle')
add('cfunc-m7.1','Measure a portable extended-precision arithmetic candidate','cfunc-m3.22 infra-m0.5',
    'In CalcFunc numeric research tooling, implement or bind a provenance-cleared ext80 arithmetic candidate for add/subtract/multiply/divide/conversion and explicit rounding. Cross-check against independently captured x87 primitive witnesses. Do not advertise Excel-equivalent transcendental functions merely because ext80 arithmetic works; keep this research backend out of the shipping Exact capability table.',
    'ext80_primitives=Independent primitive witnesses distinguish intermediate extended rounding from final binary64 rounding.||no_false_exact=The portable candidate is not selectable as a certified Exact backend without the separate target/algorithm evidence required by numeric policy.',port='numeric',scope='No software Exact product commitment and no formal proofs. Transcendental model identification and CPU-family equivalence remain independent evidence questions.')
add('infra-m7.6','Measure native target variation and software-Exact feasibility','cfunc-m7.1 cfunc-m3.24 infra-m7.4',
    'Capture held-out EXP/LN/LOG/POWER and midpoint-sensitive witnesses on available distinct x86-64 CPU families and Excel builds; run the portable candidate in actual wasm. Retain mismatches, CPU fingerprints, backend costs and missing-target coverage. Produce a decision artifact separating ext80 arithmetic agreement from transcendental algorithm/microcode agreement; unavailable machines are explicit missing evidence, not inferred parity.',
    'target_matrix=Each tested CPU/build/backend pair has independent observed bits and a reproducible mismatch count.||wasm_feasibility=Actual browser runs record correctness and cost for the portable candidate without relabeling its mismatches as Exact passes.',port='numeric',platform='oracle')
add('cfunc-m7.2','Declare measured Fast-mode tolerance contracts','cfunc-m3.25 infra-m7.4',
    'Create per-function/operator absolute/relative/ULP acceptance policies with domain and conditioning notes, derived from held-out Exact-target observations. Errors, booleans, text and shapes remain exact semantic comparisons. Distinguish numerical deviations propagated through branch decisions from harmless output distance; keep uncharacterized families visibly uncharacterized rather than giving one global epsilon.',
    'tolerance_metadata=Every numeric-sensitive callable has an explicit policy or an uncharacterized status; generated oracle paths use that one policy.||branch_drift=A small numeric difference that changes IF/comparison/error/shape behaviour is reported as a semantic divergence even when an output epsilon would hide it.',port='func')
add('infra-m7.7','Verify portable Fast determinism and numeric mode defaults','cfunc-m7.2 chost-m4.8 onecalc-m6.1',
    'Replay identical effect tapes on Windows, Linux and actual wasm with pinned portable math and numeric parsing/text policies. Verify Fast is the default for newly created workbooks and OneCalc sessions on all platforms, and Exact requires an explicitly available target. A persisted Exact request on an unsupported target prompts a choice before calculation; it never silently becomes Fast.',
    'fast_cross_target=All deterministic Fast witnesses produce the same values/types/shapes on the tested target matrix; unresolved differences block the deterministic claim.||mode_reopen=Opening Exact metadata in wasm exposes NumericModeUnavailable and leaves prior caches clearly stale until a user chooses an available mode.',port='oracle',platform='oracle')
add('ceng-m7.1','Retain the reference strategy and differential-twin harness','ceng-m5.9 ceng-m4.5',
    'Expose the permanent single-threaded reference strategy behind the calculation API and implement a differential harness that can consume a future optimized strategy. Compare evaluated nodes, dependency/read footprints, spill ownership, provenance and values under the same operations/effect tape. Test an intentionally faulty strategy in test tooling; do not introduce a second shipping graph or formula evaluator now.',
    'faulty_twin_detected=A strategy with a missed dynamic dependency fails comparison even when initial cell values match.||same_evaluator=Architecture gates prove both strategy test adapters call CalcFml rather than independent expression implementations.',port='engine')
add('infra-m7.8','Record named product-path benchmarks without thresholds','infra-m6.1 infra-m7.7 ceng-m7.1',
    'Add named cold/warm recalc, 100k fill-down sharing, 20k chains, cross-sheet edits, whole-column ranges, 100k-plus materialization, 1M-row viewport, 10k notebook, 100MB workbook-memory, SCC/TABLE/CF/DV and numeric-backend workloads. Record platform/build/mode, timings, allocations, peak memory and structural counters. The browser runs actual wasm; report compressed input versus decoded size separately. Set no speed, memory or duration targets.',
    'benchmark_artifact=Each named workload has a retained raw measurement and correctness verdict, or an explicit not-run reason.||structural_counter=An unrelated one-cell edit has no whole-sheet clone/readout in independent allocation/read instrumentation; display delta size reflects changed interests rather than sheet size.',port='engine',platform='tooling')
add('infra-m7.9','Fuzz operation sequences package parsing and cancellation','infra-m6.1 ceng-m7.1',
    'Generate bounded operation sequences across edits, insert/delete, names, mode switches, spills, undo/redo, save/open and cancellation. Add malformed ZIP/XML/relationship inputs with resource-limit classification. Compare snapshots, deltas and saved packages after each committed operation; retain minimized crash/semantic witnesses.',
    'operation_replay=Replaying a minimized sequence yields equal authored states and independently computed reference results at every checkpoint.||hostile_package=ZIP bombs, path traversal and malformed relationships produce bounded typed failures without external resource access.||cancel_consistency=Cancellation never publishes a partially calculated generation as fresh.',port='engine')
add('infra-m7.10','Publish conformance and campaign evidence through one report','infra-m7.4 infra-m7.5 infra-m7.7 infra-m7.8 infra-m7.9',
    'Generate one report from immutable manifests: catalogue scope, calculated/formula coverage, numeric target/mode, exact and Fast verdicts, feature intersections, unsupported/refused/not-run populations and benchmark links. Include known mismatches rather than hiding them in waived totals; cross-check claimed coverage against independently enumerated source workbooks and tests.',
    'report_reconciliation=All headline totals reconcile to immutable case manifests and are reproducible from a clean checkout.||honesty_regression=Injected cached-only, unsupported, skipped and divergent cases cannot increase calculated-match coverage or disappear from totals.',port='oracle',platform='tooling')

# M8: core integration can finish independently; first-release guard cannot.
add('chost-m8.1','Complete application lifecycle and cross-skin recovery','chost-m6.4 chost-m4.13 dnacalc-m6.23',
    'Finish the host lifecycle controller for multiple documents, unsaved recovery, exclusive batch mode, external update ordering and per-skin interests. Run the platform-neutral controller without a UI, and consume it unchanged through desktop and browser transports. Preserve explicit capability failures and saved numeric preferences.',
    'lifecycle_recovery=Crash/recovery and reopen across two skins preserve the last committed authored revision without duplicate operations.||serial_updates=An external update queued during an exclusive transaction becomes one ordered operation after commit and produces matching graph/document revisions.',port='host')
add('dnacalc-m8.1','Integrate all skins and commands in the shipped packages','dnacalc-m6.24 chost-m8.1 infra-m7.10',
    'Package the desktop and browser DNA Calc applications using the same application/skin controllers. Execute cross-skin editing, dependent recalc, save/reopen and accessibility scenarios through the actual distributed entrypoints. Make unavailable platform features explicit; do not substitute a test-only host for end-to-end tests.',
    'shipped_entrypoints=Installed Windows/Linux builds and the deployed browser bundle execute the same retained scenario with identical authored semantics.||switch_and_save=Notebook/tree/canvas/Excel edits coexist across save and real-Excel reopen; missing manifest metadata causes diagnosed flattening, not silent value reconstruction.',port='skins')
add('onecalc-m8.1','Verify the shipped OneCalc package stays engine-free','onecalc-m6.1 infra-m7.7',
    'Package Windows/Linux and browser OneCalc, exercise actual formula editing/binding/evaluation/formatting and numeric-mode behaviour, and inspect the complete resolved dependency closure. Document session fixture interchange separately from workbook loading; do not add CalcHost to get a convenient file dialog or provider.',
    'shipped_onecalc=The distributed app evaluates typed reference fixtures and LAMBDA/array cases using only CalcFml and CalcFunc.||engine_free=The complete native/web application dependency graphs contain no CalcEngine or CalcHost crate.',port='fml')
add('infra-m8.1','Verify core completion and mandatory phase-2 attachments','infra-m7.10 dnacalc-m8.1 onecalc-m8.1 chost-m4.13 cfunc-m4.1 cfml-m4.3 ceng-m4.6',
    'Reconcile this plan’s core scope, all UI-core rows, platform acceptance and every phase-2 attachment with retained tests and non-test call sites. Mark this checkpoint CORE COMPLETE only; chart/pivot/print and other separable implementations must remain pending until their generated task graphs actually land. Fail on missing runtime callers and stale self-reported completion flags.',
    'core_matrix=Every core requirement maps to executed retained evidence on its declared platforms.||seam_traffic=UDF registration/invalidation, late-bound dispatch, events, exclusive transactions, external/RTD and callback/range/panel attachments are invoked by actual application or command-line controllers, not test-only code.',port='oracle',platform='tooling')
add('infra-m8.2','Enforce first-version release scope including separable areas','infra-m8.1 infra-m3.20 infra-m3.21 infra-m3.22 infra-m3.23 infra-m3.24',
    'Implement a fail-closed release-scope checker that discovers the implementation graphs generated by each separable-area planning bead and verifies their executable exits. Initially it MUST fail because those areas are not built. Publish a core-ready build separately; a first-version release requires actual chart rendering, pivot refresh/GETPIVOTDATA, printing/layout and all inventoried phase-1 commands. Preserve explicit phase-2 exclusions without quietly moving phase-1 features there.',
    'planning_is_not_shipping=With every planning bead closed but no chart/pivot/print implementation evidence, the release checker still fails.||actual_release=Only retained shipped-product tests for every mandatory area allow the checker to pass; unknown inventory rows or missing child graphs block release.',port='fresh',platform='tooling',scope='This task builds the release gate, not the separable implementations. Closing this gate-implementation task must not itself mark the first-version release milestone complete.')

# Repair prerequisite details and distinguish early skin work from later full semantics.
def task(id):
    return next(t for t in TASKS if t['id']==id)
def block(id,*deps):
    t=task(id); seen={d['depends_on_id'] for d in t['dependencies']}
    for d in deps:
        if d not in seen:
            t['dependencies'].append({'depends_on_id':d,'type':'blocks'});seen.add(d)
block('chost-m4.5','cdoc-m3.6')
block('infra-m5.3','ceng-m5.9')
block('dnacalc-m6.23','ceng-m3.4','ceng-m3.2','cfml-m3.4','chost-m4.2')
block('infra-m6.1',*[t['id'] for t in TASKS if t['labels'][0]=='m6' and t['labels'][1]!='infra'])
# Bootstrap creates the runner; its own closure cannot require that runner to exist first.
for id, cmds in {
'infra-m0.1':'cargo metadata --no-deps --format-version 1\ncargo check --workspace\ncargo check --workspace --target wasm32-unknown-unknown --exclude calc-platform-desktop --exclude calc-oracle-excel --exclude xtask',
'infra-m0.2':'cargo test -p xtask --test acceptance infra_m0_2 -- --nocapture\ncargo xtask ci-audit --require-executed-targets windows,linux,wasm',
'infra-m0.3':'cargo test -p xtask --test acceptance infra_m0_3 -- --nocapture\ncargo xtask accept --id infra-m0.3 --platform windows --require-all\ncargo xtask accept --id infra-m0.3 --platform linux --require-all',
}.items():
    t=task(id)
    t['description']=re.sub(r'(## Acceptance\n```text\n).*?(\n```)',lambda m:m.group(1)+cmds+m.group(2),t['description'],count=1,flags=re.S)
# The CI bead uses the finished acceptance runner.
block('infra-m0.2','infra-m0.3')
# No mandatory iterative case can close as a not-implemented placeholder.
t=task('ceng-m5.3');t['description']=t['description'].replace('or counted explicit unsupported until the witness is implemented','with no typed-unsupported substitution for the admitted mandatory witness set')

# Early skins must not wait for structural-edit or advanced-array implementation.
t=task('chost-m2.3')
t['description']=t['description'].replace(
 'Create, move, reorder, delete and alias a block using ordinary cells, internal locator names and manifest changes in one operation batch. Keep view reorder distinct from physical worksheet move. Expose explicit reserve-space commands for spill outputs; never silently relocate neighboring formulas.',
 'Create, reorder and delete a block using ordinary cells, internal locator names and manifest changes in one operation batch. This bead moves blocks in view order only; physical worksheet moves and reserving spill space are chost-m4.2. Keep locator names distinct from optional user aliases, whose formula-binding semantics arrive in cfml-m3.1.')
t['description']=t['description'].replace('`chost_m2_3__physical_move`: The explicit move-cells command uses CalcFml structural reference transforms and records one undo transaction.','`chost_m2_3__block_transaction`: Creating or deleting a block changes its anchor, locator and manifest together in one undo transaction; view reorder changes no worksheet coordinates.')
t=task('dnacalc-m2.6');t['title']='Show formula details and dependency navigation in every skin'
t['description']=t['description'].replace('Show callables spills and dependency navigation in every skin.',t['title']+'.')
t['description']=t['description'].replace('Use shared formula detail panels for precedents/dependents, source and evaluated value. A raw LAMBDA cell retains Excel’s uninvoked-cell error presentation; offer a named-LAMBDA definition and explicit invocation inspector without inventing a non-Excel reference language. Show spill anchor versus child ownership and blockers.',
 'Use shared formula detail panels for source, actual evaluated scalar/error values and direct cell precedents/dependents in all four skins. Preserve typed provenance and diagnostics. This early bead establishes the real shared projection route; named-LAMBDA and realized spill inspection are completed by dnacalc-m3.1 after their engine support lands.')
t['description']=t['description'].replace('`dnacalc_m2_6__lambda_truth`: The same raw/named/invoked LAMBDA workbook has consistent Excel cell values and explicit callable inspection in all skins.','`dnacalc_m2_6__formula_truth`: The same scalar/reference workbook shows identical formulas, errors, provenance and calculated values in all skins through the actual host projection.')
t=task('infra-m2.1');t['description']=t['description'].replace('Create an Excel-interchange suite covering all three persistence candidates for comparison, with the chosen hybrid as production. Exercise two skins concurrently, row insertion, alias/locator rename, backing-sheet deletion, cross-block names, raw/named lambdas and spills. Include a 10000-block notebook and retain size/memory/open-time measurements without targets.',
 'Build the early persistence comparison fixtures for the three candidate designs, with only the chosen hybrid used by production. Exercise two skins concurrently, ordinary cell formulas, Excel row insertion, internal-locator rename and backing-sheet deletion. Include a 10000-block notebook and retain size/memory/open-time measurements without targets. Cross-block names, named/raw LAMBDAs and realized spills extend this same suite in infra-m3.3; do not fake them in this earlier milestone.')
t['description']=t['description'].replace('Every requested persistence case has retained pre/post Excel packages and expected recovery states.','Every early persistence case named in Do has retained pre/post Excel packages and expected recovery states; advanced cases are linked to infra-m3.3 and not marked passed here.')
add('dnacalc-m3.1','Complete named-callable and spill inspection across all skins','dnacalc-m2.6 ceng-m3.2 ceng-m3.4 cfml-m3.4 chost-m3.1',
    'Extend the existing shared detail/projection route with named-LAMBDA function blocks, explicit invocation inspection, raw-LAMBDA grid errors, spill anchors/children/blockers and range/name dependency navigation. A function block stores its canonical definition in an Excel defined name, not a duplicated executable body in both cell and manifest. The visible grid includes labels or invocation examples; editing the definition uses the same name command.',
    'lambda_truth=The same raw, defined-name and invoked LAMBDA cases have Excel-compatible grid outcomes and explicit callable inspection in all four skins.||spill_truth=Editing a real blocker changes the actual engine spill result and every interested skin through one delta; no skin fabricates the child values.',port='skins',priority=0)
add('infra-m3.3','Complete the advanced skin persistence interchange matrix','infra-m2.1 dnacalc-m3.1 cdoc-m3.1 cdoc-m3.3 chost-m2.4 infra-m1.2',
    'Extend the original three-design comparison suite with names that reference other blocks, scoped and renamed aliases, raw/named/invoked LAMBDAs, spill resizing/blocking, metadata stripping, and real-Excel edits. Retain candidate-by-case outcomes and overhead observations; only the selected hybrid ships. Prove a deleted backing sheet produces recoverable orphan diagnostics rather than cached-value resurrection.',
    'complete_persistence_matrix=Each of the three candidate designs has a recorded outcome for every case in brief section 3.5; the chosen design passes its declared interchange/recovery behaviour.||canonical_lambda=Real-Excel Name Manager edits change the function observed in every DNA skin without a stale duplicate executable definition.',port='oracle',priority=0,platform='oracle')
block('infra-m3.2','infra-m3.3','ceng-m3.9')
block('dnacalc-m6.23','dnacalc-m3.1')
# Make the thin slice self-contained rather than depending on later function families.
t=task('cfunc-m1.2');t['description']=t['description'].replace('numeric arithmetic, comparison and error propagation','numeric arithmetic, comparison, the initial SUM implementation over typed range arguments, and error propagation')
t=task('cfml-m1.1');t['description']=t['description'].replace('calls and A1 references','calls and A1 references including a simple quoted/unquoted sheet qualifier')
t=task('cfml-m1.2');t['description']=t['description'].replace('Bind A1/R1C1 cell and rectangular references','Bind same-sheet and explicitly sheet-qualified A1/R1C1 cell and rectangular references')
t=task('ceng-m5.3');t['description']=t['description'].replace('follows the captured behaviour or a counted explicit unsupported result until its witness is implemented.','follows the independently captured behaviour for every admitted mandatory witness; an unsupported substitute fails this task.')

# Admission and lane-boundary refinements.
block('cfunc-m0.1','infra-m0.4')
task('cdoc-m0.1')['description']=task('cdoc-m0.1')['description'].replace(PORTS['doc'],'fresh; design these neutral types from this plan. OxDoc model names are not copied into the bootstrap contract.')
task('cfunc-m4.1')['dependencies']=[d for d in task('cfunc-m4.1')['dependencies'] if d['depends_on_id']!='cfml-m2.3']
block('onecalc-m6.1','cfunc-m4.1','cfml-m4.3')
task('cfunc-m1.1')['description']=task('cfunc-m1.1')['description'].replace('Create a single declarative catalogue that generates IDs, metadata, dispatch, signature help and oracle selection.','Create a single logical declarative catalogue from per-family source fragments with explicit stable IDs; generate IDs, metadata, dispatch, signature help and oracle selection. Different family-port agents own different fragments, not one shared registry source file.')

# Complete the early named-function block host route before its skin consumer.
add('chost-m3.3','Implement canonical named-function block editing','chost-m2.3 cdoc-m3.1 ceng-m3.2 cfml-m2.3',
    'Add the host commands to create a named-LAMBDA block, edit its single defined-name formula, attach labels/examples and invoke an explicit inspection request. Store no second executable definition in a worksheet text cell or layout manifest. Use the existing versioned operation/undo route and publish name/dependent deltas.',
    'one_definition=Editing a function block changes exactly its authoritative defined-name formula; dependent worksheet invocations recalculate through CalcFml.||excel_name_edit=After real-Excel-compatible name edits are loaded, the block displays and invokes the new definition without resurrecting an old manifest body.',port='host',priority=0)
block('dnacalc-m3.1','chost-m3.3')
block('ceng-m3.6','cfunc-m3.19')
block('ceng-m4.2','ceng-m3.6')
block('ceng-m4.3','cfunc-m3.7')
block('ceng-m5.4','ceng-m3.6')
block('ceng-m5.7','cfunc-m3.10')
block('ceng-m5.8','cfunc-m3.21')
block('infra-m7.10','infra-m7.3','infra-m7.6')

# Priority bands: explicit P0 tasks keep their urgency; everything else is banded by milestone and
# lifted one band when it transitively blocks a large part of the graph, so a wide ready set is
# ordered by consequence instead of listing 150 tasks at P1. Adjust _BAND or the threshold freely.
_EXPLICIT_P0={t['id'] for t in TASKS if t['priority']==0}
_BAND={'m0':0,'m1':0,'m2':1,'m3':2,'m4':2,'m5':3,'m6':3,'m7':3,'m8':4}
_dependents=defaultdict(set)
for t in TASKS:
    for d in t['dependencies']:
        if d['type']=='blocks': _dependents[d['depends_on_id']].add(t['id'])
def _closure(id,seen=None):
    seen=set() if seen is None else seen
    for x in _dependents.get(id,()):
        if x not in seen: seen.add(x); _closure(x,seen)
    return seen
TRANSITIVE_DEPENDENTS={t['id']:len(_closure(t['id'])) for t in TASKS}
for t in TASKS:
    if t['id'] in _EXPLICIT_P0: continue
    band=_BAND[t['labels'][0]]
    if TRANSITIVE_DEPENDENTS[t['id']]>=30: band-=1
    t['priority']=max(1,band)

# Machine-readable coverage: all listed IDs are checked against the exported graph.
COVERAGE = {
 'milestone_exits': {
 'm0':['infra-m0.1','infra-m0.2','infra-m0.3','cfunc-m0.1','cdoc-m0.1'],
 'm1':['infra-m1.3'],
 'm2':['cfml-m2.1','cfml-m2.2','ceng-m2.1','onecalc-m2.1','infra-m2.1'],
 'm3':['infra-m3.2','infra-m3.3','ceng-m3.8','chost-m3.2'],
 'm4':['infra-m4.1','ceng-m4.6','chost-m4.10','chost-m4.11','chost-m4.12','chost-m4.13','cfml-m4.3','cfunc-m4.1'],
 'm5':['infra-m5.3'], 'm6':['infra-m6.1'], 'm7':['infra-m7.10'], 'm8':['infra-m8.1'],
 },
 'release_gate':{'implementation_task':'infra-m8.2','initial_state':'must_fail_until_separable_implementations_exist','scope':'First-version release is not equivalent to closing all current core/planning beads.'},
 'ui_core': {
 'ribbon_backstage_qat_keytips':['dnacalc-m6.1'], 'formula_bar_edit_point_ime':['cfml-m6.2','dnacalc-m6.3'],
 'selection':['dnacalc-m6.2'], 'autofill':['dnacalc-m6.4'], 'drag_move_copy':['dnacalc-m6.5'],
 'clipboard_paste_special':['dnacalc-m6.6'], 'axes_panes_groups':['dnacalc-m6.7'],
 'format_number_alignment':['dnacalc-m6.8'], 'format_font_border_fill_protection':['dnacalc-m6.9'],
 'styles_themes':['dnacalc-m6.10'], 'names_namebox':['dnacalc-m6.11'], 'conditional_formatting':['dnacalc-m6.12'],
 'data_validation':['dnacalc-m6.13'], 'find_replace_goto':['dnacalc-m6.14'], 'sort_filters':['dnacalc-m6.15'],
 'table_tools':['dnacalc-m6.16'], 'insert_delete_lifecycle':['dnacalc-m6.17'],
 'calculation_options_data_tables':['dnacalc-m6.18'], 'formula_audit':['dnacalc-m6.19'],
 'comments_notes_hyperlinks_review':['dnacalc-m6.20'], 'status_zoom_sheet_tabs_windows':['dnacalc-m6.21'],
 'keyboard_accessibility':['dnacalc-m6.22','dnacalc-m6.24'], 'undo_redo':['chost-m2.2','chost-m4.9','dnacalc-m6.3'],
 },
 'ui_separable': {'charts_drawings':['infra-m3.20'], 'pivots':['infra-m3.21'], 'printing_page_layout':['infra-m3.22'], 'external_analytics':['infra-m3.23'], 'extended_desktop_inventory':['infra-m3.24']},
 'phase2_seams': {
 'udf_registry':['cfunc-m4.1','cfml-m4.3','chost-m4.12','onecalc-m6.1'],
 'async_volatility_thread_safety_caller':['cfunc-m4.1','cfml-m4.3','ceng-m4.6'],
 'object_dispatch':['chost-m4.12'], 'events_macro_transaction':['chost-m4.11','chost-m8.1'],
 'external_rtd_updates':['ceng-m4.6','chost-m4.10','chost-m4.11'],
 'xll_callback_attachment':['chost-m4.13'], 'range_picker':['chost-m4.13','dnacalc-m6.22'],
 'ide_panel_attachment':['chost-m4.13','dnacalc-m6.19','dnacalc-m6.22'],
 },
 'numeric_requirements': {
 'ownership_persistence_defaults':['chost-m4.8','infra-m7.7'], 'metadata':['cfunc-m3.22','cfunc-m7.2'],
 'full_recalc':['ceng-m4.4'], 'native_exact':['cfunc-m3.24','infra-m0.5'],
 'software80_feasibility':['cfunc-m7.1','infra-m7.6'], 'fast_portability':['cfunc-m3.23','infra-m7.7'],
 'scoring':['infra-m7.4','cfunc-m7.2'],
 },
 'skin_persistence_cases': {
 'three_design_comparison':['infra-m2.1','infra-m3.3'], 'two_skins':['infra-m2.1'],
 'real_excel_row_name_sheet_edits':['infra-m2.1','infra-m3.3'], 'cross_block_names':['infra-m3.3'],
 'lambda':['chost-m3.3','dnacalc-m3.1','infra-m3.3'], 'spills':['dnacalc-m3.1','infra-m3.3'],
 'ten_thousand_blocks':['infra-m2.1','infra-m7.8'],
 },
 'non_excel_skins': {
 'notebook':['dnacalc-m2.2','dnacalc-m3.1','dnacalc-m6.23'],
 'tree':['dnacalc-m2.3','dnacalc-m3.1','dnacalc-m6.23'],
 'canvas':['dnacalc-m2.4','dnacalc-m3.1','dnacalc-m6.23'],
 },
 'separable_attachments': {
 'chart':['ceng-m3.8','cdoc-m3.5','chost-m3.1','chost-m3.2','infra-m3.20'],
 'pivot':['ceng-m3.8','cdoc-m3.5','chost-m3.2','infra-m3.21'],
 'print':['cdoc-m3.5','chost-m3.2','infra-m3.22'],
 'external':['ceng-m3.7','ceng-m3.8','cdoc-m3.4','cdoc-m3.5','chost-m3.2','infra-m3.23'],
 'extended_ui':['infra-m3.1','chost-m3.2','chost-m4.13','infra-m3.24'],
 },
 'measurements': ['infra-m0.6','infra-m2.1','infra-m3.3','infra-m5.1','infra-m5.2','infra-m7.2','infra-m7.4','infra-m7.6','infra-m7.7','infra-m7.8','infra-m7.10'],
 'independent_lanes_after_bootstrap': ['cfunc-m1.1','cfml-m1.1','cdoc-m1.1','ceng-m1.1'],
 'future_area_graphs': ['infra-m3.20','infra-m3.21','infra-m3.22','infra-m3.23','infra-m3.24'],
}

SOURCE_AUDIT = {
 'method':'Original authoring run (2026-10-05): selected connected-GitHub source reads; no builds, live Excel execution or access to owner-local files/caches; blob identities do not assert a coherent multi-repository commit set. Owner-side review (2026-10-12): every path cited by a port task was checked to exist in the local checkout under C:/Work/DnaCalc, repository HEADs and uncommitted changes were recorded, and the br import was executed; still no builds or Excel runs.',
 'sources': [
 ('OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs','7a8aba4e30a35da3ba4505ca469408a81ac4d617','1-240','inspected; native assembly and unresolved NT source-tree provenance citation','turn14file0'),
 ('OxFml/crates/oxfml_core/src/eval/mod.rs','33b990513e33867a29bed1421336aac2af63fa41','1-260;660-870;900-1300','inspected private compile/frame/callable paths; not complete evaluator audit','turn16file0;turn19file0;turn30file0;turn31file0'),
 ('OxFunc/crates/oxfunc_value_types/src/lib.rs','d79060c9fb57d6620482644dfe2b2a29d11b8ede','1-160','inspected feature-dependent Arc/Rc value and reference types','turn22file0'),
 ('OxFunc/crates/oxfunc_core/src/function_call.rs','426cf6877cc60ce74caddc55a66bb287510baaec','1-190','inspected resolved-call metadata and scratch; not all invoke implementations','turn23file0'),
 ('OxFml/crates/oxfml_core/Cargo.toml','5fecfaacfb12e67843509769dd31c77d334e4c40','whole file','inspected default dependency features','turn32file0'),
 ('OxFunc/crates/oxfunc_core/Cargo.toml','2077e7162d37b33b0a255322fbf91a24a4bc8f54','whole file','inspected send-values default','turn33file0'),
 ('OxCalc/src/oxcalc-core/src/grid/machine/r1c1_plan.rs','f22100cb79bb262803df6e82e28e19734b9892a5','1-250','inspected actual second evaluator','turn20file0'),
 ('OxCalc/src/oxcalc-core/src/grid/reference_engine.rs','4895febda98aaeb230055da40553fd31daf7b78b','1-165','inspected profile/provider and per-sheet/cross-sheet maps','turn28file0'),
 ('OxFml/crates/oxfml_core/src/binding/profile.rs','b5d7caccdab8b9c772722160723920ee6896f143','1-200','inspected multiple profiles and normal-form identity','turn24file0'),
 ('OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs','8e4e65ef67502ac50a2bd91d66f400145173b6d8','1-200','inspected dependency taxonomy, not all closure consumers','turn21file0'),
 ('DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs','1a2382d82b003a43654252d651a719c4d8bfe508','1-150','inspected old host ownership/imports','turn18file0'),
 ('DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs','31f96e2abaf0d6b13caa7eda083288777355aca8','1-180','inspected render-plan/canvas/Leptos/editor integration; no visual tests run','turn25file0'),
 ('Dicklesworthstone/beads_rust/src/model/mod.rs','c7e91c11aa81055d625af13e72efd58511d03aa4','1-240;400-720;850-980','inspected native Issue/Dependency serialization fields','turn26file0;turn27file0;turn29file0'),
 ],
 'unavailable': [
 {'path':'OxFunc/smart-fuzzer/cache and runs','disposition':'Present locally (1,618 and 26,774 files, untracked and not git-ignored, owner-reported about 3.4 GB); not backed up and not re-measured by either review. infra-m0.6 performs the backup.'},
 ],
 'verified_locally_2026_10_12': [
 {'path':'OxDoc/crates/oxdoc-model/src/lib.rs','disposition':'Present: 5,939 lines, one file; 191 OxCalc-named symbols. The authoring run could not fetch it; the port text now names it.'},
 {'path':'OxDoc/crates/oxdoc-xlsx/src/lib.rs','disposition':'Present: 37,648 lines, one file; RoundTripRefusal taxonomy, WriteMode::FreshExport, W010 tranche-2 authoring (cell add, formula add/remove/change, sheet lifecycle) committed 2026-09-15 with further uncommitted edits.'},
 {'path':'Foundation/notes/PROGRAM_INVESTIGATION_2026-09-10.md','disposition':'Present: the 2026-09-10 investigation whose lessons the brief restates; its dual-profile direction is superseded by the Excel-only decision of 2026-10-05.'},
 {'path':'OxCalc/src/oxcalc-core/src/consumer.rs','disposition':'Present: about 1.46 MB with uncommitted changes; still not read in full, and still not a port source.'},
 {'path':'All other Port-from paths','disposition':'Every file and directory named in PORTS and in the function/UI family tasks exists locally; sizes are listed in ROLLOUT_BRIEF.md.'},
 ],
 'repository_state_2026_10_12': {
 'OxFunc':'6fca3518 (2026-09-29) clean','OxFml':'e2b6149 (2026-09-15) 1 uncommitted file (eval/mod.rs)','OxCalc':'5ea74767 (2026-09-15) 6 uncommitted files incl. consumer.rs','OxDoc':'baf05f9 (2026-09-15) 2 modified + 2 untracked (cell-add authoring)','DnaTreeCalc':'a1e486d (2026-09-15) .beads only','DnaOneCalc':'61e3979 (2026-07-12) archived into DnaTreeCalc','OxReplay':'0b90052 (2026-09-02)','OxXlPlay':'adcdb51 (2026-09-02)','OxVba':'4145280e (2026-09-22)','ExcelFunctionsHandbook':'30c5b03 (2026-09-24)','DnaVisiCalc':'7047389 (2026-03-08)','Foundation':'dd36664 (2026-10-05) clean, branch master',
 },
 'inferences': [
 'Arc-default OxFunc and Rc-handle sampled OxFml suggest a source-version compatibility problem; not compiled or reproduced.',
 'Function/Doc migration decisions beyond sampled files are conditional on source pin/admission tasks.',
 ],
 'owner_reported_not_remeasured':['Repository line counts','Parity ledger counts','Corpus size and coverage','Untracked evidence size','Historical performance baselines'],
}
SOURCE_AUDIT['sources']=[dict(path=p,blob_sha=h,source_line_ranges=r,disposition=d) for p,h,r,d,c in SOURCE_AUDIT['sources']]
brief = ROOT / 'calc-reimplementation-planning-gpt6pro.md'
if brief.exists(): SOURCE_AUDIT['uploaded_brief_sha256']=hashlib.sha256(brief.read_bytes()).hexdigest()

from graphlib import TopologicalSorter

def export():
    # Completion cannot skip the explicit functional milestone witnesses.
    for milestone, exits in COVERAGE['milestone_exits'].items():
        if milestone != 'm8':
            block('infra-m8.1',*exits)
    ids={t['id'] for t in TASKS}
    assert len(ids)==len(TASKS)
    for t in TASKS:
        for d in t['dependencies']:
            if d['type']=='blocks': assert d['depends_on_id'] in ids,(t['id'],d)
    topo=list(TopologicalSorter({t['id']:{d['depends_on_id'] for d in t['dependencies'] if d['type']=='blocks'} for t in TASKS}).static_order())
    groups=defaultdict(list)
    for t in TASKS: groups[f'{t["labels"][1]}-{t["labels"][0]}'].append(t)
    epics=[]
    for id,children in groups.items():
        prefix,m=id.split('-');lib=LIBS[prefix][0]
        desc=f'''## Outcome
All child tasks for {lib} in {MILESTONES[m]} have their required observable acceptance evidence.

## Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

## Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

## Port from
Not applicable; exact source/admission instructions are in each child task.

## Acceptance
Inspect the executed evidence of all {len(children)} child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

## Platforms
The union of the child tasks' declared target matrices.

## Out of scope
Additional implementation work or silently changing the owner's scope.
'''
        epics.append(dict(id=id,title=f'Complete {lib} — {m.upper()}',issue_type='epic',priority=min(t['priority'] for t in children),labels=[m,prefix,'epic'],dependencies=[],description=desc))
    def sortkey(t):
        m=int(t['labels'][0][1:]);prefix=t['labels'][1];liborder=list(LIBS).index(prefix)
        num=0 if t['issue_type']=='epic' else int(t['id'].split('.')[-1])
        return m,liborder,num
    records=sorted(epics+TASKS,key=sortkey)
    (ROOT/'beads.plan.jsonl').write_text(''.join(json.dumps(t,ensure_ascii=False)+'\n' for t in records),encoding='utf-8')
    stamp=datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds').replace('+00:00','Z')
    idmap={t['id']:t['id'] for t in records}
    native=[]
    for t in records:
        n=dict(t);n['id']=idmap[t['id']];n.update(status='open',created_at=stamp,updated_at=stamp,created_by='DNA Calc planning export',source_system='dnacalc-reimplementation-plan',compaction_level=0,external_ref='dnacalc-plan/'+t['id'],notes='Provisional plan ID: '+t['id']+'. Native export preserves this ID; acceptance command IDs and test names use the same stable plan identity.')
        n['dependencies']=[dict(issue_id=n['id'],depends_on_id=idmap[d['depends_on_id']],type=d['type'],created_at=stamp,created_by='DNA Calc planning export',metadata=None) for d in t['dependencies']]
        native.append(n)
    (ROOT/'beads.br.jsonl').write_text(''.join(json.dumps(t,ensure_ascii=False)+'\n' for t in native),encoding='utf-8')
    (ROOT/'br-id-map.json').write_text(json.dumps(idmap,indent=2)+'\n',encoding='utf-8')
    (ROOT/'coverage.json').write_text(json.dumps(COVERAGE,indent=2)+'\n',encoding='utf-8')
    (ROOT/'source-audit.json').write_text(json.dumps(SOURCE_AUDIT,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    orderlines=['# Executable task order (one valid topological ordering)','', 'This is not a schedule. Independent ready tasks may run in parallel. Epics are grouping records.','']
    orderlines += [f'{i}. `{id}` — {task(id)["title"]}' for i,id in enumerate(topo,1)]
    (ROOT/'topological-order.md').write_text('\n'.join(orderlines)+'\n',encoding='utf-8')
    out=['# Complete core bead graph','',f'**{len(TASKS)} tasks · {len(epics)} epics · {len(records)} total records.** Full descriptions and named acceptance cases are in `beads.plan.jsonl`.','',
         'An arrow `←` names blocking prerequisites. Epic membership is listed by the grouping and is not a blocking wait for epic closure.','']
    for m,title in MILESTONES.items():
        out += [f'## {m.upper()}: {title}','']
        for prefix in LIBS:
            id=f'{prefix}-{m}'
            if id not in groups: continue
            out += [f'### `{id}` — {LIBS[prefix][0]}','']
            for t in sorted(groups[id],key=sortkey):
                ds=[d['depends_on_id'] for d in t['dependencies'] if d['type']=='blocks']
                deps=', '.join('`'+x+'`' for x in ds) or 'none'
                out += [f'- **`{t["id"]}`** {t["title"]} (P{t["priority"]}) ← {deps}']
            out += ['']
    outline='\n'.join(out)+'\n';(ROOT/'BEAD_GRAPH.md').write_text(outline,encoding='utf-8')
    details=['# Complete bead descriptions','','These descriptions are identical to `beads.plan.jsonl`. Acceptance commands are future implementation requirements, not completed tests.','']
    for t in records:
        blocks=[d['depends_on_id'] for d in t['dependencies'] if d['type']=='blocks']
        parent=[d['depends_on_id'] for d in t['dependencies'] if d['type']=='parent-child']
        details += [f'## {t["id"]} — {t["title"]}','',f'**Type:** {t["issue_type"]} · **Priority:** {t["priority"]} · **Labels:** {", ".join(t["labels"])}','',
                    '**Blocking prerequisites:** '+(', '.join('`'+b+'`' for b in blocks) if blocks else 'none')+'.',
                    '**Parent epic:** '+('`'+parent[0]+'`' if parent else 'none')+'.','']
        details += ['#'+l if l.startswith('## ') else l for l in t['description'].rstrip('\n').splitlines()]
        details += ['','---','']
    (ROOT/'BEAD_DETAILS.md').write_text('\n'.join(details)+'\n',encoding='utf-8')
    plan=ROOT/'DNA_CALC_REIMPLEMENTATION_PLAN.md'
    text=plan.read_text(encoding='utf-8')
    # The standalone outline uses H1; demote headings for the integrated graph section.
    integrated='\n'.join('#'+l if l.startswith('#') else l for l in outline.splitlines()[1:])
    text=re.sub(r'<!-- GENERATED_OUTLINE -->.*?(?=\n## Evidence references)', '<!-- GENERATED_OUTLINE -->\n\n'+integrated+'\n', text,flags=re.S)
    plan.write_text(text,encoding='utf-8')
    return records

if __name__=='__main__':
    rs=export();print(f'Exported {len(TASKS)} tasks and {len(rs)-len(TASKS)} epics.');print('task priorities:',dict(sorted(Counter(t['priority'] for t in TASKS).items())))
