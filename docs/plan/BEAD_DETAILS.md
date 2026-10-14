# Complete bead descriptions

These descriptions are identical to `beads.plan.jsonl`. Acceptance commands are future implementation requirements, not completed tests.

## infra-m0 — Complete Infrastructure — M0

**Type:** epic · **Priority:** 0 · **Labels:** m0, infra, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for Infrastructure in Bootstrap checkpoint — independent lanes, CI and evidence safety have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 6 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## infra-m0.1 — Create the monorepo and isolated crate workspaces

**Type:** task · **Priority:** 0 · **Labels:** m0, infra, tooling

**Blocking prerequisites:** none.
**Parent epic:** `infra-m0`.

### Outcome
Create the monorepo and isolated crate workspaces. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Bootstrap checkpoint — independent lanes, CI and evidence safety. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Create one Cargo workspace with calc-value, calc-func, calc-fml, calc-doc-model, calc-doc, calc-engine, calc-host, dna-onecalc, dna-calc, xtask and platform adapter packages. Use one lockfile; every CI job and every agent checkout has its own target directory. Pre-create per-library tests and acceptance-manifest directories so four agents do not edit a shared registry. Commit the plan bundle (this plan, BEAD_GRAPH.md, BEAD_DETAILS.md, the JSONL exports, coverage.json and validate_plan.py) under docs/plan/ so every agent reads the text that bead Context sections cite, and commit the .beads/ tracker. Write AGENTS.md as the agent operating contract: claim a bead with br ready or bv --robot-next and br update <id> --status in_progress; run its acceptance and keep the receipt; one commit per bead on master with the bead ID in the message; br close with the evidence; file a follow-up bead instead of widening scope; all work on master as pull, work on one bead, commit, push, with no branches and no worktrees, ever; an agent working concurrently with another uses its own full clone of the repository on master (which gives it its own target directory), pulls before starting, and resolves a rejected push by pulling master and re-running its acceptance before pushing again; never two cargo invocations in one target directory at the same time; and the honesty rules from the brief (provenance before value, count every fallback, no faked behaviour, no seams without traffic, measure rather than assert).
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh

### Acceptance
```text
cargo metadata --no-deps --format-version 1
cargo check --workspace
cargo check --workspace --target wasm32-unknown-unknown --exclude calc-platform-desktop --exclude calc-oracle-excel --exclude xtask
```
- `infra_m0_1__workspace_builds`: Every portable package builds on the native targets and wasm32-unknown-unknown; prohibited OS imports fail the architecture test.
- `infra_m0_1__independent_lanes`: Four separate clones on master build Func, Fml, Doc and Engine modules concurrently, each in its own target directory, with no shared file writes.
- `infra_m0_1__agent_contract`: AGENTS.md names the claim, acceptance, commit and close commands, the master-only pull/work/commit/push rule with no branches or worktrees, and the one-cargo-per-target-directory rule, and docs/plan/ contains the plan text and bead exports that the bead descriptions cite.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m0.2 — Install Windows Linux and browser CI

**Type:** task · **Priority:** 0 · **Labels:** m0, infra, tooling

**Blocking prerequisites:** `infra-m0.1`, `infra-m0.3`.
**Parent epic:** `infra-m0`.

### Outcome
Install Windows Linux and browser CI. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Bootstrap checkpoint — independent lanes, CI and evidence safety. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Add required native test jobs, actual wasm-bindgen/browser test execution in Chromium and Firefox, desktop packaging smoke jobs and source/license checks. Pin toolchains and action revisions; archive logs and artifacts. Do not treat a wasm cargo check as executed tests.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh

### Acceptance
```text
cargo test -p xtask --test acceptance infra_m0_2 -- --nocapture
cargo xtask ci-audit --require-executed-targets windows,linux,wasm
```
- `infra_m0_2__browser_failure_is_red`: A deliberately failing browser fixture makes CI fail rather than report a compile-only success.
- `infra_m0_2__platform_matrix`: Every portable crate and both app cores appear in a machine-readable executed-target manifest.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m0.3 — Build the acceptance runner and architecture gates

**Type:** task · **Priority:** 0 · **Labels:** m0, infra, tooling

**Blocking prerequisites:** `infra-m0.1`.
**Parent epic:** `infra-m0`.

### Outcome
Build the acceptance runner and architecture gates. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Bootstrap checkpoint — independent lanes, CI and evidence safety. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Implement cargo xtask accept --id ID --platform windows|linux|wasm|excel --require-all. Collect per-package manifests; validate test enumeration, run commands, and fail on missing/zero/skipped required cases. Add cargo-metadata dependency gates for OneCalc, skins and platform crates; deny tree-reference tokens and production-only dead seams using a reviewed call-site manifest plus integration coverage.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh

### Acceptance
```text
cargo test -p xtask --test acceptance infra_m0_3 -- --nocapture
cargo xtask accept --id infra-m0.3 --platform windows --require-all
cargo xtask accept --id infra-m0.3 --platform linux --require-all
```
- `infra_m0_3__zero_tests_fail`: A misspelled required test name fails the runner.
- `infra_m0_3__forbidden_dependency`: Adding calc-engine to OneCalc or calc-host to CalcFml is rejected mechanically.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m0.4 — Pin legacy sources and resolve inaccessible migration inputs

**Type:** task · **Priority:** 0 · **Labels:** m0, infra, tooling

**Blocking prerequisites:** `infra-m0.1`.
**Parent epic:** `infra-m0`.

### Outcome
Pin legacy sources and resolve inaccessible migration inputs. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Bootstrap checkpoint — independent lanes, CI and evidence safety. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Record commit IDs, working-tree state and blob hashes for every legacy repository under the DnaCalc root (OxFunc, OxFml, OxCalc, OxDoc, DnaTreeCalc, the archived DnaOneCalc, OxReplay, OxXlPlay, ExcelFunctionsHandbook, DnaVisiCalc, Foundation) and the exact files/symbols selected by each port task. All of these are available locally; the 2026-10-12 review found uncommitted changes in OxCalc (consumer.rs and three grid/machine files), OxDoc (oxdoc-xlsx lib.rs plus an untracked cell-add conformance fixture), OxFml (eval/mod.rs) and DnaTreeCalc (.beads only): commit or stash them in those repositories first so every pinned blob is a committed object. Record parser, function-family, corpus and oracle-runner file lists before copying. Mark any module that is genuinely absent as unavailable and route its tasks to fresh implementation.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh

### Acceptance
```text
cargo xtask accept --id infra-m0.4 --platform windows --require-all
cargo xtask accept --id infra-m0.4 --platform linux --require-all
```
- `infra_m0_4__source_manifest`: Every selected port input has a repository, commit, path, content hash, license status and read disposition, and no pinned blob comes from an uncommitted working tree.
- `infra_m0_4__missing_source`: A port input absent from the pinned manifest is reported as unavailable and never counted as a verified reusable implementation.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m0.5 — Quarantine questionable numeric provenance

**Type:** task · **Priority:** 0 · **Labels:** m0, infra, tooling

**Blocking prerequisites:** `infra-m0.4`.
**Parent epic:** `infra-m0`.

### Outcome
Quarantine questionable numeric provenance. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Bootstrap checkpoint — independent lanes, CI and evidence safety. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Trace the provenance of OxFunc/crates/oxfunc_core/src/excel_numeric/ without consulting leaked or disassembled sources. x87.rs (890 lines, 21 asm! blocks) states that its EXP/LN/LOG/POWER sequences follow fpw32/tran/i386/87tran.asm and the 87disp.asm control word from the NT CRT source tree, while its ATAN path cites W111 black-box witnesses; mod.rs dispatches x86_64 versus portable; tables.rs with gen_tables.py, gammaln.rs, research.rs and x87_excel_ground_truth.tsv need a per-file disposition. Keep questionable code out of the new tree. Retain an independent black-box witness set with acquisition provenance; create a reviewed disposition for each candidate module. Clearing a comment is not clearance.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs: exp, pow_direct, atan and the fyl2x_fn expansion are QUARANTINED, not port sources until provenance clearance. OxFunc/crates/oxfunc_core/src/excel_numeric/ is a discovery location only. Use independent public-ISA/black-box witnesses for replacement code.

### Acceptance
```text
cargo xtask accept --id infra-m0.5 --platform windows --require-all
cargo xtask accept --id infra-m0.5 --platform linux --require-all
```
- `infra_m0_5__quarantine_gate`: A source manifest entry marked unresolved or tainted cannot enter the release source allowlist.
- `infra_m0_5__independent_witness`: Every retained numeric witness links to black-box acquisition metadata and no quarantined implementation is used to generate expected bits.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m0.6 — Back up and content-address existing oracle evidence

**Type:** task · **Priority:** 0 · **Labels:** m0, infra, tooling

**Blocking prerequisites:** `infra-m0.4`.
**Parent epic:** `infra-m0`.

### Outcome
Back up and content-address existing oracle evidence. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Bootstrap checkpoint — independent lanes, CI and evidence safety. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
From the owner-accessible checkout inventory OxFunc/smart-fuzzer/cache (1,618 files) and OxFunc/smart-fuzzer/runs (26,774 files), owner-reported at about 3.4 GB, untracked and not git-ignored; hash, deduplicate and copy to a versioned artifact store with a second verified backup. Keep manifests and minimal witnesses in git, bulk blobs outside git. Restore a random manifest-selected sample and record acquisition metadata gaps instead of fabricating them.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m0.6 --platform windows --require-all
cargo xtask accept --id infra-m0.6 --platform linux --require-all
```
- `infra_m0_6__restore_roundtrip`: Restored blobs hash-identically to the local originals and both backup inventories reconcile.
- `infra_m0_6__unknown_metadata`: Legacy data without an Excel fingerprint is labelled unknown and cannot certify a new Exact target.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m0 — Complete CalcFunc — M0

**Type:** epic · **Priority:** 0 · **Labels:** m0, cfunc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcFunc in Bootstrap checkpoint — independent lanes, CI and evidence safety have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cfunc-m0.1 — Define portable values IDs and numerical policy

**Type:** task · **Priority:** 0 · **Labels:** m0, cfunc, all

**Blocking prerequisites:** `infra-m0.1`, `infra-m0.4`.
**Parent epic:** `cfunc-m0`.

### Outcome
Define portable values IDs and numerical policy. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Bootstrap checkpoint — independent lanes, CI and evidence safety. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Implement calc-value: workbook/sheet/name/table/node generational IDs, CellKey with u32 row and column, scalar/array/error/reference/callable values, NumericMode and NumericTarget. Use Arc-owned immutable payloads and Send+Sync callable contracts. Checked grid coordinates cover the full Excel bounds without allocating cells.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_value_types/src/lib.rs: CalcValue, CoreValue, Shared, ReferenceLike and WorksheetErrorCode. Remove the Rc feature arm and string-address payloads; preserve value semantics.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m0_1 -- --nocapture
cargo xtask accept --id cfunc-m0.1 --platform windows --require-all
cargo xtask accept --id cfunc-m0.1 --platform linux --require-all
cargo xtask accept --id cfunc-m0.1 --platform wasm --require-all
```
- `cfunc_m0_1__send_sync`: Compile-time assertions prove public immutable values and plan-facing reference tokens are Send+Sync with no Rc opt-out.
- `cfunc_m0_1__grid_edges`: A1 and XFD1048576 round-trip; zero/out-of-bounds coordinates fail and distinct sheet IDs cannot alias.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m0 — Complete CalcDoc — M0

**Type:** epic · **Priority:** 0 · **Labels:** m0, cdoc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcDoc in Bootstrap checkpoint — independent lanes, CI and evidence safety have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cdoc-m0.1 — Define engine-neutral authored operations and revisions

**Type:** task · **Priority:** 0 · **Labels:** m0, cdoc, all

**Blocking prerequisites:** `infra-m0.1`, `cfunc-m0.1`.
**Parent epic:** `cdoc-m0`.

### Outcome
Define engine-neutral authored operations and revisions. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Bootstrap checkpoint — independent lanes, CI and evidence safety. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
In calc-doc-model define AuthoredRevision, immutable model roots, WorkbookOp batches, structural-edit descriptors, CellInput and document-only metadata. The operation schema contains no engine/session types and never serializes computed cache as authored input. Generate per-capability operation codecs.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh; design these neutral types from this plan. OxDoc model names are not copied into the bootstrap contract.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m0_1 -- --nocapture
cargo xtask accept --id cdoc-m0.1 --platform windows --require-all
cargo xtask accept --id cdoc-m0.1 --platform linux --require-all
cargo xtask accept --id cdoc-m0.1 --platform wasm --require-all
```
- `cdoc_m0_1__op_roundtrip`: Every initially admitted operation round-trips through the versioned wire codec.
- `cdoc_m0_1__no_engine_dependency`: The model package compiles without CalcEngine, CalcFml or CalcHost and a cached value cannot be decoded as an authored formula replacement.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m1 — Complete Infrastructure — M1

**Type:** epic · **Priority:** 0 · **Labels:** m1, infra, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for Infrastructure in First functional milestone — real Excel open/recalc/edit/save/reopen have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 3 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## infra-m1.1 — Unify cache and live-oracle verdicts

**Type:** task · **Priority:** 0 · **Labels:** m1, infra, all

**Blocking prerequisites:** `infra-m0.3`, `cfunc-m0.1`.
**Parent epic:** `infra-m1`.

### Outcome
Unify cache and live-oracle verdicts. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Implement a canonical CaptureRecord and CellVerdict library shared by file-cache replay and live Excel capture. Track formula denominator, calculated count, correct count, stale/not-modeled/missing/uncertified categories and type-aware comparisons. Decimal <v> is an observation, not a guaranteed binary64 oracle; mark strict-bit certainty separately.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m1.1 --platform windows --require-all
cargo xtask accept --id infra-m1.1 --platform linux --require-all
cargo xtask accept --id infra-m1.1 --platform wasm --require-all
```
- `infra_m1_1__cached_value_trap`: A file-cache-only result is counted as not calculated even when numerically equal to the expectation.
- `infra_m1_1__denominator_integrity`: Hidden sheets, shared formulas and error-valued formulas remain in the enumerated denominator.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m1.2 — Implement one isolated Excel COM capture adapter

**Type:** task · **Priority:** 0 · **Labels:** m1, infra, oracle

**Blocking prerequisites:** `infra-m1.1`, `infra-m0.4`.
**Parent epic:** `infra-m1`.

### Outcome
Implement one isolated Excel COM capture adapter. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Consolidate one Windows adapter for opening fixtures with macros/add-ins/external links disabled, setting calculation policy, forcing recalc, capturing raw Value2 number bits and formula/type/settings metadata, saving and reopening. Capture Excel build, compatibility version, CPU, OS, locale, date system and calculation settings. Legacy runners become readers/converters, not alternate capturers.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m1.2 --platform windows --require-all
cargo xtask accept --id infra-m1.2 --platform linux --require-all
cargo xtask accept --id infra-m1.2 --platform wasm --require-all
cargo xtask accept --id infra-m1.2 --platform excel --require-all
```
- `infra_m1_2__capture_replays`: The same capture produces identical verdict records on Windows, Linux and wasm.
- `infra_m1_2__untrusted_workbook`: Macro and external-link activation remain disabled and a capture timeout terminates only the isolated worker.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m1.3 — Prove the first real workbook round trip

**Type:** task · **Priority:** 0 · **Labels:** m1, infra, oracle

**Blocking prerequisites:** `chost-m1.2`, `infra-m1.2`.
**Parent epic:** `infra-m1`.

### Outcome
Prove the first real workbook round trip. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Add a licensed Excel-authored two-sheet fixture with constants, SUM, a dependency chain, a deliberately perturbed cache and an unrelated opaque part. Run the product HostState/CalcFml path, compare provenance-first, edit a value and add a formula, save, and reopen in real Excel with no repair. Retain input/output packages, raw capture and verdict artifact.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m1.3 --platform windows --require-all
cargo xtask accept --id infra-m1.3 --platform linux --require-all
cargo xtask accept --id infra-m1.3 --platform wasm --require-all
cargo xtask accept --id infra-m1.3 --platform excel --require-all
```
- `infra_m1_3__end_to_end`: All formula cells in this declared fixture are independently enumerated, actually calculated and compared; the deliberate stale cache is caught.
- `infra_m1_3__excel_reopen`: Real Excel opens the saved package without repair and calculates the changed and added formulas to the captured expectations.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m1 — Complete CalcFunc — M1

**Type:** epic · **Priority:** 0 · **Labels:** m1, cfunc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcFunc in First functional milestone — real Excel open/recalc/edit/save/reopen have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 2 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cfunc-m1.1 — Generate the initial function catalogue and call interface

**Type:** task · **Priority:** 0 · **Labels:** m1, cfunc, all

**Blocking prerequisites:** `cfunc-m0.1`, `infra-m0.3`.
**Parent epic:** `cfunc-m1`.

### Outcome
Generate the initial function catalogue and call interface. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Create a single logical declarative catalogue from per-family source fragments with explicit stable IDs; generate IDs, metadata, dispatch, signature help and oracle selection. Different family-port agents own different fragments, not one shared registry source file. Add FunctionCallTarget::invoke with caller-owned EvalServices and scratch. Admit arithmetic, comparison, concatenation, SUM and IF metadata for the thin slice; expose lazy argument policy rather than eager-evaluating IF.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/function.rs: FunctionMeta; crates/oxfunc_core/src/function_call.rs: FunctionCallTarget, FunctionCallScratch, FunctionExecutionContextBundle; crates/oxfunc_core/src/registry.rs; crates/oxfunc_core/src/functions/surface_dispatch.rs. Paths after the first are relative to OxFunc/.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m1_1 -- --nocapture
cargo xtask accept --id cfunc-m1.1 --platform windows --require-all
cargo xtask accept --id cfunc-m1.1 --platform linux --require-all
cargo xtask accept --id cfunc-m1.1 --platform wasm --require-all
```
- `cfunc_m1_1__catalogue_consistency`: Dispatch, help and arity data enumerate exactly the same admitted IDs.
- `cfunc_m1_1__unknown_function`: Unknown IDs return a typed registration error without dispatch by a second string switch.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m1.2 — Port base coercions and scalar arithmetic

**Type:** task · **Priority:** 0 · **Labels:** m1, cfunc, all

**Blocking prerequisites:** `cfunc-m1.1`, `infra-m0.4`.
**Parent epic:** `cfunc-m1`.

### Outcome
Port base coercions and scalar arithmetic. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port primitive worksheet coercion, empty versus omitted versus empty-text distinctions, numeric arithmetic, comparison, the initial SUM implementation over typed range arguments, and error propagation through the generated call interface. Preserve signed-zero policy explicitly and pass NumericMode to all arithmetic sites.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/function.rs: FunctionMeta; crates/oxfunc_core/src/function_call.rs: FunctionCallTarget, FunctionCallScratch, FunctionExecutionContextBundle; crates/oxfunc_core/src/registry.rs; crates/oxfunc_core/src/functions/surface_dispatch.rs. Paths after the first are relative to OxFunc/.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m1_2 -- --nocapture
cargo xtask accept --id cfunc-m1.2 --platform windows --require-all
cargo xtask accept --id cfunc-m1.2 --platform linux --require-all
cargo xtask accept --id cfunc-m1.2 --platform wasm --require-all
```
- `cfunc_m1_2__mixed_operands`: Retained Excel witnesses distinguish direct text/boolean arguments from referenced values and assert exact error codes.
- `cfunc_m1_2__error_order`: Competing operand errors return the observed error in the observed order.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m1 — Complete CalcFml — M1

**Type:** epic · **Priority:** 0 · **Labels:** m1, cfml, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcFml in First functional milestone — real Excel open/recalc/edit/save/reopen have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 3 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cfml-m1.1 — Port the lossless Excel lexer and basic parser

**Type:** task · **Priority:** 0 · **Labels:** m1, cfml, all

**Blocking prerequisites:** `infra-m0.1`, `infra-m0.3`, `infra-m0.4`, `cfunc-m0.1`.
**Parent epic:** `cfml-m1`.

### Outcome
Port the lossless Excel lexer and basic parser. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Create the Excel-only syntax tree for literals, unary/binary operators, calls and A1 references including a simple quoted/unquoted sheet qualifier; retain whitespace, trivia, incomplete input and UTF-16 editor offsets. Remove host-reference/tree grammar at entry, not as a later profile rejection.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/syntax/ and OxFml/crates/oxfml_core/src/language_service/ are discovery paths. The source-manifest bead must resolve exact lexer/parser/service/test files and exported symbols before porting. Preserve lossless source spans; do not copy host-reference productions.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m1_1 -- --nocapture
cargo xtask accept --id cfml-m1.1 --platform windows --require-all
cargo xtask accept --id cfml-m1.1 --platform linux --require-all
cargo xtask accept --id cfml-m1.1 --platform wasm --require-all
```
- `cfml_m1_1__lossless_roundtrip`: Formatting, quoted strings and incomplete input reproduce their original source text.
- `cfml_m1_1__tree_syntax_rejected`: A legacy host-reference token gets a precise unsupported-syntax diagnostic and never a tree AST node.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m1.2 — Implement typed reference binding without the engine

**Type:** task · **Priority:** 0 · **Labels:** m1, cfml, all

**Blocking prerequisites:** `cfml-m1.1`, `cfunc-m1.1`.
**Parent epic:** `cfml-m1`.

### Outcome
Implement typed reference binding without the engine. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Bind same-sheet and explicitly sheet-qualified A1/R1C1 cell and rectangular references against a ReferenceCatalog trait and caller anchor; produce typed relative/absolute operands. Introduce ReferenceReader returning borrowed/chunked values with missing/blank distinctions. OneCalc can implement these traits without an engine.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/reference_engine.rs: StrictExcelGridReferenceProfile, ExcelGridReferenceSystemProvider; OxFml/crates/oxfml_core/src/binding/profile.rs: reference requests/normal-form identity. Extract Excel semantics, not profiles or BTreeMap<String,...> providers.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m1_2 -- --nocapture
cargo xtask accept --id cfml-m1.2 --platform windows --require-all
cargo xtask accept --id cfml-m1.2 --platform linux --require-all
cargo xtask accept --id cfml-m1.2 --platform wasm --require-all
```
- `cfml_m1_2__relative_absolute`: Mixed dollar references instantiate correctly at two callers and reject references crossing grid bounds.
- `cfml_m1_2__provider_reads`: An independently instrumented provider sees typed requests, not constructed address strings.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m1.3 — Expose reusable compiled plans and evaluation frames

**Type:** task · **Priority:** 0 · **Labels:** m1, cfml, all

**Blocking prerequisites:** `cfml-m1.2`, `cfunc-m1.2`.
**Parent epic:** `cfml-m1`.

### Outcome
Expose reusable compiled plans and evaluation frames. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Implement compile, instantiate and evaluate APIs with Arc<CompiledPlan>, caller-owned PlanCache, EvalScratch and per-call EvalFrame. Separate immutable instructions from mutable lexical frames. Route every function through CalcFunc and every reference through the typed reader.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/eval/mod.rs: compiled expressions and evaluation; OxFml/crates/oxfml_core/src/binding/profile.rs: ReferenceNormalFormKey, ReferenceFingerprintPolicy. Discard ReferencePolicy, host-reference syntax and the tree profile.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m1_3 -- --nocapture
cargo xtask accept --id cfml-m1.3 --platform windows --require-all
cargo xtask accept --id cfml-m1.3 --platform linux --require-all
cargo xtask accept --id cfml-m1.3 --platform wasm --require-all
```
- `cfml_m1_3__reuse_plan`: Two evaluations of one compiled plan produce different values after provider data changes without invoking compile again.
- `cfml_m1_3__lazy_if`: IF does not evaluate the unselected error/volatile branch, as verified by provider-call counters.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m1 — Complete CalcEngine — M1

**Type:** epic · **Priority:** 0 · **Labels:** m1, ceng, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcEngine in First functional milestone — real Excel open/recalc/edit/save/reopen have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 2 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## ceng-m1.1 — Create workbook-wide graph storage and literal input access

**Type:** task · **Priority:** 0 · **Labels:** m1, ceng, all

**Blocking prerequisites:** `infra-m0.3`, `cfunc-m0.1`, `cdoc-m0.1`.
**Parent epic:** `ceng-m1`.

### Outcome
Create workbook-wide graph storage and literal input access. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Implement caller-owned EngineState with interned node IDs, one workbook graph, formula-node records and borrowed immutable input pages. Stage calculation-relevant operation effects through prepare_edit. Keep source bytes, formatting and editable cells outside the engine.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridAxisValueDependency, GridAxisVisibilityDependency, GridSheetSpanDependency and reverse range indexing. Reimplement with integer IDs in one workbook-wide graph; do not inherit the old per-sheet scheduler.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m1_1 -- --nocapture
cargo xtask accept --id ceng-m1.1 --platform windows --require-all
cargo xtask accept --id ceng-m1.1 --platform linux --require-all
cargo xtask accept --id ceng-m1.1 --platform wasm --require-all
```
- `ceng_m1_1__two_sheets_one_graph`: Nodes on two sheets share one graph and cannot collide by coordinates.
- `ceng_m1_1__input_sharing`: Literal input pages are referenced from the document snapshot without a second owned value copy.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m1.2 — Calculate the thin slice through CalcFml

**Type:** task · **Priority:** 0 · **Labels:** m1, ceng, all

**Blocking prerequisites:** `ceng-m1.1`, `cfml-m1.3`.
**Parent epic:** `ceng-m1`.

### Outcome
Calculate the thin slice through CalcFml. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Build dependencies for direct references and ranges, compute dirty closure and a deterministic topological ready queue, and evaluate each selected node through CalcFml. Publish a staged ValueDelta with Calculated{revision,tick,mode,target} provenance only after successful finish.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m1_2 -- --nocapture
cargo xtask accept --id ceng-m1.2 --platform windows --require-all
cargo xtask accept --id ceng-m1.2 --platform linux --require-all
cargo xtask accept --id ceng-m1.2 --platform wasm --require-all
```
- `ceng_m1_2__chain_recalculates`: Changing a literal recalculates a multi-sheet dependent chain and the observed values match independent fixture expectations.
- `ceng_m1_2__wrong_cache_not_pass`: A wrong file cache is not reused or scored as calculated; injected evaluation failure leaves the previous published generation intact.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m1 — Complete CalcDoc — M1

**Type:** epic · **Priority:** 0 · **Labels:** m1, cdoc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcDoc in First functional milestone — real Excel open/recalc/edit/save/reopen have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 3 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cdoc-m1.1 — Implement bounded ZIP package reading and raw preservation

**Type:** task · **Priority:** 0 · **Labels:** m1, cdoc, all

**Blocking prerequisites:** `infra-m0.3`, `cdoc-m0.1`, `infra-m0.4`.
**Parent epic:** `cdoc-m1`.

### Outcome
Implement bounded ZIP package reading and raw preservation. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Open an .xlsx OPC package into a seekable backing source, a part directory and lazy XML readers. Reject zip-slip, external entity expansion and declared-size bombs under explicit resource limits. Retain untouched compressed payloads and relationship/content-type information.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m1_1 -- --nocapture
cargo xtask accept --id cdoc-m1.1 --platform windows --require-all
cargo xtask accept --id cdoc-m1.1 --platform linux --require-all
cargo xtask accept --id cdoc-m1.1 --platform wasm --require-all
```
- `cdoc_m1_1__raw_preservation`: An independently read hash of every untouched part payload is unchanged after a no-op save.
- `cdoc_m1_1__malformed_zip`: Path traversal and adversarial expansion fail with typed errors and no filesystem writes.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m1.2 — Load scalar cells formulas and file-cache provenance

**Type:** task · **Priority:** 0 · **Labels:** m1, cdoc, all

**Blocking prerequisites:** `cdoc-m1.1`.
**Parent epic:** `cdoc-m1`.

### Outcome
Load scalar cells formulas and file-cache provenance. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Parse workbook/sheet relationships, cell addresses, inline/shared strings, numeric/error/boolean cells, basic styles, formulas and cached <v> separately. Materialize only occupied cell records; tag all loaded formula caches FileCached with source-part identity.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m1_2 -- --nocapture
cargo xtask accept --id cdoc-m1.2 --platform windows --require-all
cargo xtask accept --id cdoc-m1.2 --platform linux --require-all
cargo xtask accept --id cdoc-m1.2 --platform wasm --require-all
```
- `cdoc_m1_2__authored_vs_cache`: A formula whose cached value is deliberately wrong retains the formula and visibly FileCached provenance.
- `cdoc_m1_2__sparse_extent`: A workbook containing only XFD1048576 does not allocate a dense worksheet.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m1.3 — Author cell changes and write a repair-free thin workbook

**Type:** task · **Priority:** 0 · **Labels:** m1, cdoc, all

**Blocking prerequisites:** `cdoc-m1.2`.
**Parent epic:** `cdoc-m1`.

### Outcome
Author cell changes and write a repair-free thin workbook. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Implement value/formula add, replace and clear for existing sheets; maintain worksheet dimensions, row order, shared-string indices and formula cache invalidation. Build prepare_save and streaming write with a typed refusal path; preserve unsupported untouched parts.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m1_3 -- --nocapture
cargo xtask accept --id cdoc-m1.3 --platform windows --require-all
cargo xtask accept --id cdoc-m1.3 --platform linux --require-all
cargo xtask accept --id cdoc-m1.3 --platform wasm --require-all
```
- `cdoc_m1_3__formula_authoring`: Add a new formula to a formerly absent cell and independently inspect its XML before Excel reopen.
- `cdoc_m1_3__save_refusal`: A refusal produces no partially accepted source image and leaves the caller-owned model revision unchanged.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m1 — Complete CalcHost — M1

**Type:** epic · **Priority:** 0 · **Labels:** m1, chost, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcHost in First functional milestone — real Excel open/recalc/edit/save/reopen have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 2 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## chost-m1.1 — Commit authored and engine edits as one transaction

**Type:** task · **Priority:** 0 · **Labels:** m1, chost, all

**Blocking prerequisites:** `cdoc-m1.3`, `ceng-m1.2`.
**Parent epic:** `chost-m1`.

### Outcome
Commit authored and engine edits as one transaction. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Create HostState owning the document and EngineState. Translate a command to one WorkbookOp batch; prepare document and engine effects, preflight reservations, then commit both without fallible allocation. Roll back preparation on error; publish one authored revision and a dirty set.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m1_1 -- --nocapture
cargo xtask accept --id chost-m1.1 --platform windows --require-all
cargo xtask accept --id chost-m1.1 --platform linux --require-all
cargo xtask accept --id chost-m1.1 --platform wasm --require-all
```
- `chost_m1_1__atomic_prepare_failure`: Failure after document preparation leaves both original revisions and all cell contents unchanged.
- `chost_m1_1__recalc_failure_keeps_edit`: A committed edit survives a later recalc failure while old computed values become Stale.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m1.2 — Expose open recalc save and delta projections

**Type:** task · **Priority:** 0 · **Labels:** m1, chost, all

**Blocking prerequisites:** `chost-m1.1`.
**Parent epic:** `chost-m1`.

### Outcome
Expose open recalc save and delta projections. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Wire open → document model → engine projection; expose dispatch/step/project/save APIs and a thin headless calcctl caller. Keep save snapshots immutable and clear dirty status only when the acknowledged save revision still equals the current authored revision.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m1_2 -- --nocapture
cargo xtask accept --id chost-m1.2 --platform windows --require-all
cargo xtask accept --id chost-m1.2 --platform linux --require-all
cargo xtask accept --id chost-m1.2 --platform wasm --require-all
```
- `chost_m1_2__delta_only`: A one-cell edit returns only changed cells and affected metadata, verified by serialized delta contents.
- `chost_m1_2__concurrent_save_edit`: Completing a save of revision r after editing r+1 does not mark r+1 saved.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## onecalc-m1 — Complete DNA OneCalc — M1

**Type:** epic · **Priority:** 0 · **Labels:** m1, onecalc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for DNA OneCalc in First functional milestone — real Excel open/recalc/edit/save/reopen have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## onecalc-m1.1 — Ship the engine-free OneCalc application controller

**Type:** task · **Priority:** 0 · **Labels:** m1, onecalc, all

**Blocking prerequisites:** `cfml-m1.3`, `infra-m0.3`.
**Parent epic:** `onecalc-m1`.

### Outcome
Ship the engine-free OneCalc application controller. The behaviour is available through the DNA OneCalc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within First functional milestone — real Excel open/recalc/edit/save/reopen. Own only DNA OneCalc files for this task; consume prerequisite APIs rather than editing another library. Production route: the OneCalc application controller with its in-memory reference provider, never CalcEngine.

### Do
Build OneCalc controller plus a minimal desktop/browser surface with formula text, typed inputs, result, diagnostics and trace. Implement a small typed workbook/sheet/name provider for references; keep CalcEngine and CalcHost absent even from transitive dependencies.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/eval/mod.rs: compiled expressions and evaluation; OxFml/crates/oxfml_core/src/binding/profile.rs: ReferenceNormalFormKey, ReferenceFingerprintPolicy. Discard ReferencePolicy, host-reference syntax and the tree profile.

### Acceptance
```text
cargo test -p dna-onecalc --test acceptance onecalc_m1_1 -- --nocapture
cargo xtask accept --id onecalc-m1.1 --platform windows --require-all
cargo xtask accept --id onecalc-m1.1 --platform linux --require-all
cargo xtask accept --id onecalc-m1.1 --platform wasm --require-all
```
- `onecalc_m1_1__reference_without_engine`: A formula reads two explicitly supplied cells and calculates correctly with only CalcFml/CalcFunc.
- `onecalc_m1_1__dependency_gate`: cargo metadata for the full OneCalc package graph contains neither calc-engine nor calc-host.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m2 — Complete Infrastructure — M2

**Type:** epic · **Priority:** 0 · **Labels:** m2, infra, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for Infrastructure in Shared formulas and all four skins over one workbook have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## infra-m2.1 — Prove cross-skin xlsx persistence with Excel edits

**Type:** task · **Priority:** 0 · **Labels:** m2, infra, oracle

**Blocking prerequisites:** `dnacalc-m2.6`, `chost-m2.4`, `infra-m1.2`.
**Parent epic:** `infra-m2`.

### Outcome
Prove cross-skin xlsx persistence with Excel edits. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Build the early persistence comparison fixtures for the three candidate designs, with only the chosen hybrid used by production. Exercise two skins concurrently, ordinary cell formulas, Excel row insertion, internal-locator rename and backing-sheet deletion. Include a 10000-block notebook and retain size/memory/open-time measurements without targets. Cross-block names, named/raw LAMBDAs and realized spills extend this same suite in infra-m3.3; do not fake them in this earlier milestone.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m2.1 --platform windows --require-all
cargo xtask accept --id infra-m2.1 --platform linux --require-all
cargo xtask accept --id infra-m2.1 --platform wasm --require-all
cargo xtask accept --id infra-m2.1 --platform excel --require-all
```
- `infra_m2_1__interchange_matrix`: Every early persistence case named in Do has retained pre/post Excel packages and expected recovery states; advanced cases are linked to infra-m3.3 and not marked passed here.
- `infra_m2_1__ten_thousand_blocks`: All 10000 block IDs reconcile and formulas remain ordinary Excel cells; measured overhead is reported rather than declared fast.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m2 — Complete CalcFml — M2

**Type:** epic · **Priority:** 0 · **Labels:** m2, cfml, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcFml in Shared formulas and all four skins over one workbook have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 6 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cfml-m2.1 — Canonicalize R1C1 templates and binding-context keys

**Type:** task · **Priority:** 0 · **Labels:** m2, cfml, all

**Blocking prerequisites:** `cfml-m1.3`.
**Parent epic:** `cfml-m2`.

### Outcome
Canonicalize R1C1 templates and binding-context keys. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Normalize parsed formula structure, not raw strings. Key templates by typed R1C1 offsets/absolutes, lexical names, function/compatibility catalogue generation and semantic syntax settings. Separate binding instantiation keys for sheet/name/table contexts. Do not include the entire authored revision or caller row in template identity.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/eval/mod.rs: compiled expressions and evaluation; OxFml/crates/oxfml_core/src/binding/profile.rs: ReferenceNormalFormKey, ReferenceFingerprintPolicy. Discard ReferencePolicy, host-reference syntax and the tree profile.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m2_1 -- --nocapture
cargo xtask accept --id cfml-m2.1 --platform windows --require-all
cargo xtask accept --id cfml-m2.1 --platform linux --require-all
cargo xtask accept --id cfml-m2.1 --platform wasm --require-all
```
- `cfml_m2_1__fill_down_shares`: 100000 R1C1-equivalent formulas share one parse/compiled template, measured by an independent compile observer.
- `cfml_m2_1__different_scope_not_alias`: Identical text in two sheets with shadowing local names uses correct distinct bindings without losing template sharing.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m2.2 — Invalidate reusable plans by semantic dependencies

**Type:** task · **Priority:** 0 · **Labels:** m2, cfml, all

**Blocking prerequisites:** `cfml-m2.1`.
**Parent epic:** `cfml-m2`.

### Outcome
Invalidate reusable plans by semantic dependencies. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Maintain a caller-owned plan arena and lookup-dependency fingerprints for scoped names, tables, function registry and structural transforms. Evict unreachable plans with explicit ownership; never recompile in evaluate. Bound cache retention by active handles and configurable memory limits without changing semantics.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/eval/mod.rs: compiled expressions and evaluation; OxFml/crates/oxfml_core/src/binding/profile.rs: ReferenceNormalFormKey, ReferenceFingerprintPolicy. Discard ReferencePolicy, host-reference syntax and the tree profile.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m2_2 -- --nocapture
cargo xtask accept --id cfml-m2.2 --platform windows --require-all
cargo xtask accept --id cfml-m2.2 --platform linux --require-all
cargo xtask accept --id cfml-m2.2 --platform wasm --require-all
```
- `cfml_m2_2__edit_reuses_plan`: A value-only edit preserves plan identity across a full evaluation.
- `cfml_m2_2__rename_rebinds`: Renaming/redefining a referenced scoped name invalidates only relevant binding instances and produces the correct result.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m2.3 — Compile LET LAMBDA closures and invocation

**Type:** task · **Priority:** 0 · **Labels:** m2, cfml, all

**Blocking prerequisites:** `cfml-m2.1`, `cfunc-m1.1`.
**Parent epic:** `cfml-m2`.

### Outcome
Compile LET LAMBDA closures and invocation. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Port lexical scope analysis into immutable instructions with frame slots. Distinguish lexical value captures from workbook-name/reference lookup at invocation; expose opaque Arc-backed callable values. Implement an explicit evaluation stack/cancellation checks rather than native stack growth on wasm.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/eval/mod.rs: compiled expressions and evaluation; OxFml/crates/oxfml_core/src/binding/profile.rs: ReferenceNormalFormKey, ReferenceFingerprintPolicy. Discard ReferencePolicy, host-reference syntax and the tree profile.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m2_3 -- --nocapture
cargo xtask accept --id cfml-m2.3 --platform windows --require-all
cargo xtask accept --id cfml-m2.3 --platform linux --require-all
cargo xtask accept --id cfml-m2.3 --platform wasm --require-all
```
- `cfml_m2_3__closure_scope`: Nested LET/LAMBDA shadowing and reference captures match retained Excel witnesses.
- `cfml_m2_3__bounded_recursion`: Recursive names terminate with the specified resource/error outcome on native and wasm rather than stack overflow.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m2.4 — Port metadata-driven lazy control forms

**Type:** task · **Priority:** 0 · **Labels:** m2, cfml, all

**Blocking prerequisites:** `cfml-m2.3`.
**Parent epic:** `cfml-m2`.

### Outcome
Port metadata-driven lazy control forms. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Implement IF, IFS, CHOOSE, SWITCH, IFERROR and IFNA evaluation scheduling from each function’s observed metadata; do not assume all branch-like functions short-circuit identically. Preserve array-context branch semantics and error order with corpus witnesses.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/function.rs: FunctionMeta laziness and argument-demand metadata; OxFml/crates/oxfml_core/src/eval/mod.rs: the existing lazy IFS/CHOOSE/SWITCH/IFNA scheduling (OxFml W079, bead fml-kt8.1) as the behavioural reference; OxFml/crates/oxfml_core/src/semantics/ for argument classification. Port the metadata-driven scheduling, not the frame-local Rc/RefCell plumbing.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m2_4 -- --nocapture
cargo xtask accept --id cfml-m2.4 --platform windows --require-all
cargo xtask accept --id cfml-m2.4 --platform linux --require-all
cargo xtask accept --id cfml-m2.4 --platform wasm --require-all
```
- `cfml_m2_4__branch_observation`: Provider call traces match the witnessed evaluation order for each control form.
- `cfml_m2_4__array_branches`: Array predicates and branch shape/error combinations match the retained Excel results.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m2.5 — Expose incremental editing and formula-language services

**Type:** task · **Priority:** 0 · **Labels:** m2, cfml, all

**Blocking prerequisites:** `cfml-m2.1`.
**Parent epic:** `cfml-m2`.

### Outcome
Expose incremental editing and formula-language services. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Port completion, signature help, function/name diagnostics and lossless incremental edits into an explicit EditSession. Return UTF-16 spans and cancellation/version tags; reuse the function catalogue and the same binder as final compile.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/syntax/ and OxFml/crates/oxfml_core/src/language_service/ are discovery paths. The source-manifest bead must resolve exact lexer/parser/service/test files and exported symbols before porting. Preserve lossless source spans; do not copy host-reference productions.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m2_5 -- --nocapture
cargo xtask accept --id cfml-m2.5 --platform windows --require-all
cargo xtask accept --id cfml-m2.5 --platform linux --require-all
cargo xtask accept --id cfml-m2.5 --platform wasm --require-all
```
- `cfml_m2_5__edit_sequence`: Typing, deleting and completing a mixed-reference formula preserves source and the final compiled result.
- `cfml_m2_5__stale_response`: A service reply for an old editor revision is rejected by the consuming controller.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m2.6 — Own number-format parsing and numeric text policy

**Type:** task · **Priority:** 0 · **Labels:** m2, cfml, all

**Blocking prerequisites:** `cfml-m1.3`.
**Parent epic:** `cfml-m2`.

### Outcome
Own number-format parsing and numeric text policy. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Create CalcFml’s number-format AST and formatting service for General, numeric, date/time, text sections, conditions, colors and locale tokens. Define NumberTextService in a lower contract crate so CalcFunc TEXT/value-parsing delegates back without a dependency cycle. Keep editable entry parsing separate from locale-invariant file numeric serialization.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/locale_format.rs: LocaleFormatContext; OxFml/crates/oxfml_core/src/format/ (resolve exact files in source manifest); OxFml/crates/oxfml_core/src/eval/mod.rs: locale use at function invocation. Do not move formatting into CalcEngine.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m2_6 -- --nocapture
cargo xtask accept --id cfml-m2.6 --platform windows --require-all
cargo xtask accept --id cfml-m2.6 --platform linux --require-all
cargo xtask accept --id cfml-m2.6 --platform wasm --require-all
```
- `cfml_m2_6__text_consistency`: TEXT, formula result display and cell formatting share the same witnessed rounding policy.
- `cfml_m2_6__file_locale`: Comma-decimal UI entry saves locale-invariant numeric XML and reopens unchanged under another locale.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m2 — Complete CalcEngine — M2

**Type:** epic · **Priority:** 0 · **Labels:** m2, ceng, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcEngine in Shared formulas and all four skins over one workbook have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## ceng-m2.1 — Instantiate shared formula regions without a second evaluator

**Type:** task · **Priority:** 0 · **Labels:** m2, ceng, all

**Blocking prerequisites:** `ceng-m1.2`, `cfml-m2.2`.
**Parent epic:** `ceng-m2`.

### Outcome
Instantiate shared formula regions without a second evaluator. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Store Arc<CompiledPlan> plus caller position/binding IDs per formula or repeated region. Use compressed repeated-formula input regions while preserving per-cell result/dependency identity. Delete any temptation to special-case arithmetic or aggregates in CalcEngine.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m2_1 -- --nocapture
cargo xtask accept --id ceng-m2.1 --platform windows --require-all
cargo xtask accept --id ceng-m2.1 --platform linux --require-all
cargo xtask accept --id ceng-m2.1 --platform wasm --require-all
```
- `ceng_m2_1__shared_region`: 100000 equivalent fill-down formulas use the public plan path and calculate distinct caller-relative values.
- `ceng_m2_1__no_side_evaluator`: An injected CalcFunc witness is observed identically through OneCalc and engine evaluation; architecture scans reject local operator dispatch.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m2 — Complete CalcDoc — M2

**Type:** epic · **Priority:** 0 · **Labels:** m2, cdoc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcDoc in Shared formulas and all four skins over one workbook have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cdoc-m2.1 — Persist view manifests and stable locator names

**Type:** task · **Priority:** 0 · **Labels:** m2, cdoc, all

**Blocking prerequisites:** `cdoc-m1.3`.
**Parent epic:** `cdoc-m2`.

### Outcome
Persist view manifests and stable locator names. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Add a versioned DNA CustomXML manifest, opaque block IDs and hidden internal defined names resolving to visible backing cells/ranges. Store formulas and prose in ordinary sheet cells; store order/tree/canvas/view state only in the manifest. Preserve unknown manifest fields on compatible edits.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m2_1 -- --nocapture
cargo xtask accept --id cdoc-m2.1 --platform windows --require-all
cargo xtask accept --id cdoc-m2.1 --platform linux --require-all
cargo xtask accept --id cdoc-m2.1 --platform wasm --require-all
```
- `cdoc_m2_1__manifest_roundtrip`: Block IDs, visible formulas, prose and four view layouts survive save/open without a sidecar file.
- `cdoc_m2_1__unknown_version`: An unsupported manifest version preserves workbook semantics and produces a recovery warning rather than deleting cells.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m2 — Complete CalcHost — M2

**Type:** epic · **Priority:** 0 · **Labels:** m2, chost, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcHost in Shared formulas and all four skins over one workbook have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 4 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## chost-m2.1 — Define capability-organized skin intents and subscriptions

**Type:** task · **Priority:** 0 · **Labels:** m2, chost, all

**Blocking prerequisites:** `chost-m1.2`, `cdoc-m2.1`.
**Parent epic:** `chost-m2`.

### Outcome
Define capability-organized skin intents and subscriptions. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Replace the tree-first projection with Workbook, Selection, Formula, BlockLayout, Object, Diagnostics and Viewport capability APIs. Provide schema-version negotiation, receipts, stable object IDs and range/dependency/block subscriptions. Drive the contract with all four skin controllers from its first public version.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-skin-ir/ and DnaTreeCalc/src/dnacalc-shell/src/stage.rs: StageSurface. The source-manifest bead resolves exact intent/projection symbols. Replace tree-first WorkspaceState and the monolithic intent enum.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m2_1 -- --nocapture
cargo xtask accept --id chost-m2.1 --platform windows --require-all
cargo xtask accept --id chost-m2.1 --platform linux --require-all
cargo xtask accept --id chost-m2.1 --platform wasm --require-all
```
- `chost_m2_1__four_clients`: Grid, notebook, tree and canvas controllers issue commands through the same dispatcher against one HostState.
- `chost_m2_1__projection_isolation`: Changing a canvas position changes view metadata only and triggers no formula rewrite or recalc.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m2.2 — Implement canonical commands and undo journal

**Type:** task · **Priority:** 0 · **Labels:** m2, chost, all

**Blocking prerequisites:** `chost-m2.1`.
**Parent epic:** `chost-m2`.

### Outcome
Implement canonical commands and undo journal. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Create generated command IDs, argument schemas, capability checks and command receipts. Record inverse authored patches and view changes in one transaction journal; coalesce text-edit commits, paste and fill according to explicit gesture boundaries. Separate editor-buffer undo from workbook undo.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m2_2 -- --nocapture
cargo xtask accept --id chost-m2.2 --platform windows --require-all
cargo xtask accept --id chost-m2.2 --platform linux --require-all
cargo xtask accept --id chost-m2.2 --platform wasm --require-all
```
- `chost_m2_2__undo_redo`: An edit made in one skin is undone and redone from another with matching authored and engine revisions.
- `chost_m2_2__batch_granularity`: A multi-cell paste has one undo step; cancelled editor typing has none.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m2.3 — Implement block editing on ordinary worksheet anchors

**Type:** task · **Priority:** 0 · **Labels:** m2, chost, all

**Blocking prerequisites:** `chost-m2.2`.
**Parent epic:** `chost-m2`.

### Outcome
Implement block editing on ordinary worksheet anchors. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Create, reorder and delete a block using ordinary cells, internal locator names and manifest changes in one operation batch. This bead moves blocks in view order only; physical worksheet moves and reserving spill space are chost-m4.2. Keep locator names distinct from optional user aliases, whose formula-binding semantics arrive in cfml-m3.1.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-notebook/ (partial implementation; source manifest resolves files); DnaTreeCalc/docs/ux/FRONTEND_UI_DESIGN_AND_ROUTEMAP.md F.2/F.4 (design candidate, not authority). New persistence is specified in plan §8.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m2_3 -- --nocapture
cargo xtask accept --id chost-m2.3 --platform windows --require-all
cargo xtask accept --id chost-m2.3 --platform linux --require-all
cargo xtask accept --id chost-m2.3 --platform wasm --require-all
```
- `chost_m2_3__reorder_preserves_formula`: Notebook/tree view reorder does not change formula identity or Excel references.
- `chost_m2_3__block_transaction`: Creating or deleting a block changes its anchor, locator and manifest together in one undo transaction; view reorder changes no worksheet coordinates.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m2.4 — Reconcile non-Excel layouts after real Excel edits

**Type:** task · **Priority:** 0 · **Labels:** m2, chost, all

**Blocking prerequisites:** `chost-m2.3`.
**Parent epic:** `chost-m2`.

### Outcome
Reconcile non-Excel layouts after real Excel edits. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Resolve block locator names after row insertion, defined-name rename, sheet rename and deletion. Use explicit orphan/ambiguous statuses; never guess between duplicate targets. If manifest is stripped, present ordinary workbook cells and offer a deterministic derived layout without inventing recovered prose.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-notebook/ (partial implementation; source manifest resolves files); DnaTreeCalc/docs/ux/FRONTEND_UI_DESIGN_AND_ROUTEMAP.md F.2/F.4 (design candidate, not authority). New persistence is specified in plan §8.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m2_4 -- --nocapture
cargo xtask accept --id chost-m2.4 --platform windows --require-all
cargo xtask accept --id chost-m2.4 --platform linux --require-all
cargo xtask accept --id chost-m2.4 --platform wasm --require-all
```
- `chost_m2_4__excel_row_insert`: Locator names updated by Excel keep the same block IDs attached to the intended cells.
- `chost_m2_4__deleted_sheet`: Deleting the backing sheet yields orphan metadata and #REF witnesses, not resurrected numeric values.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## onecalc-m2 — Complete DNA OneCalc — M2

**Type:** epic · **Priority:** 0 · **Labels:** m2, onecalc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for DNA OneCalc in Shared formulas and all four skins over one workbook have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## onecalc-m2.1 — Add OneCalc editing traces formats and callable fixtures

**Type:** task · **Priority:** 0 · **Labels:** m2, onecalc, all

**Blocking prerequisites:** `onecalc-m1.1`, `cfml-m2.3`, `cfml-m2.5`, `cfml-m2.6`.
**Parent epic:** `onecalc-m2`.

### Outcome
Add OneCalc editing traces formats and callable fixtures. The behaviour is available through the DNA OneCalc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only DNA OneCalc files for this task; consume prerequisite APIs rather than editing another library. Production route: the OneCalc application controller with its in-memory reference provider, never CalcEngine.

### Do
Wire the shared formula-language service, formatting pane, callable invocation controls and typed provider fixture editor into OneCalc. Permit multi-sheet reference examples through a trivial provider without adopting a dependency graph.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/eval/mod.rs: compiled expressions and evaluation; OxFml/crates/oxfml_core/src/binding/profile.rs: ReferenceNormalFormKey, ReferenceFingerprintPolicy. Discard ReferencePolicy, host-reference syntax and the tree profile.

### Acceptance
```text
cargo test -p dna-onecalc --test acceptance onecalc_m2_1 -- --nocapture
cargo xtask accept --id onecalc-m2.1 --platform windows --require-all
cargo xtask accept --id onecalc-m2.1 --platform linux --require-all
cargo xtask accept --id onecalc-m2.1 --platform wasm --require-all
```
- `onecalc_m2_1__complete_formula_path`: LET/LAMBDA, error handling and TEXT run through the same public API as the engine.
- `onecalc_m2_1__provider_not_engine`: Changing a fixture re-evaluates the selected formula directly and the dependency gate still excludes CalcEngine.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m2 — Complete DNA Calc — M2

**Type:** epic · **Priority:** 0 · **Labels:** m2, dnacalc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for DNA Calc in Shared formulas and all four skins over one workbook have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 6 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## dnacalc-m2.1 — Build shared shell and desktop/browser transport

**Type:** task · **Priority:** 0 · **Labels:** m2, dnacalc, all

**Blocking prerequisites:** `chost-m2.1`, `infra-m0.2`.
**Parent epic:** `dnacalc-m2`.

### Outcome
Build shared shell and desktop/browser transport. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Keep Leptos for UI and Tauri for desktop packaging. Browser HostState lives in one worker; desktop HostState lives on one native owner thread. Use the same versioned transport, cancellation and delta protocol. Keep viewport rendering responsive while recalc advances in bounded cooperative steps.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-skin-ir/ and DnaTreeCalc/src/dnacalc-shell/src/stage.rs: StageSurface. The source-manifest bead resolves exact intent/projection symbols. Replace tree-first WorkspaceState and the monolithic intent enum.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m2_1 -- --nocapture
cargo xtask accept --id dnacalc-m2.1 --platform windows --require-all
cargo xtask accept --id dnacalc-m2.1 --platform linux --require-all
cargo xtask accept --id dnacalc-m2.1 --platform wasm --require-all
```
- `dnacalc_m2_1__same_receipts`: The same operation log produces identical receipts and values over native and browser transports.
- `dnacalc_m2_1__edit_during_calc`: UI selection/scroll events remain serviceable during recalc and a newer edit cancels/restarts publication safely.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m2.2 — Implement the notebook skin editor

**Type:** task · **Priority:** 0 · **Labels:** m2, dnacalc, all

**Blocking prerequisites:** `dnacalc-m2.1`, `chost-m2.3`, `cfml-m2.5`.
**Parent epic:** `dnacalc-m2`.

### Outcome
Implement the notebook skin editor. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Build virtualized code/prose blocks with formula editing, output/error/provenance display, insert/delete/reorder, markdown preview and keyboard traversal. Run a cell requests calculation of its Excel dependencies; visual order is not an execution kernel. Read/write only Host commands.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-notebook/ (partial implementation; source manifest resolves files); DnaTreeCalc/docs/ux/FRONTEND_UI_DESIGN_AND_ROUTEMAP.md F.2/F.4 (design candidate, not authority). New persistence is specified in plan §8.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m2_2 -- --nocapture
cargo xtask accept --id dnacalc-m2.2 --platform windows --require-all
cargo xtask accept --id dnacalc-m2.2 --platform linux --require-all
cargo xtask accept --id dnacalc-m2.2 --platform wasm --require-all
```
- `dnacalc_m2_2__notebook_product`: Typing a formula into a notebook block updates an ordinary backing cell and a dependent grid cell.
- `dnacalc_m2_2__prose_roundtrip`: Markdown source and order survive xlsx save/reopen and remain readable in real Excel.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m2.3 — Implement the tree editor skin

**Type:** task · **Priority:** 0 · **Labels:** m2, dnacalc, all

**Blocking prerequisites:** `dnacalc-m2.1`, `chost-m2.3`, `cfml-m2.5`.
**Parent epic:** `dnacalc-m2`.

### Outcome
Implement the tree editor skin. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Build a virtualized outline with folders, blocks, inline labels/formulas, collapse/expand, drag reparenting and dependency navigation. Parent/child is view metadata only; references always use Excel names/addresses. Show values, errors, provenance and spill ownership alongside nodes.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-skin-ir/ and DnaTreeCalc/src/dnacalc-shell/src/stage.rs: StageSurface. The source-manifest bead resolves exact intent/projection symbols. Replace tree-first WorkspaceState and the monolithic intent enum.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m2_3 -- --nocapture
cargo xtask accept --id dnacalc-m2.3 --platform windows --require-all
cargo xtask accept --id dnacalc-m2.3 --platform linux --require-all
cargo xtask accept --id dnacalc-m2.3 --platform wasm --require-all
```
- `dnacalc_m2_3__reparent_is_view`: Moving a node to another parent changes no formula or calculation result.
- `dnacalc_m2_3__cycle_in_layout`: A drag that would create a UI hierarchy cycle is rejected without affecting the workbook graph.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m2.4 — Implement the free-form canvas skin

**Type:** task · **Priority:** 0 · **Labels:** m2, dnacalc, all

**Blocking prerequisites:** `dnacalc-m2.1`, `chost-m2.3`, `cfml-m2.5`.
**Parent epic:** `dnacalc-m2`.

### Outcome
Implement the free-form canvas skin. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Build a culled 2D scene of formula/prose cards with pan/zoom, drag/resize, selection and keyboard-accessible card traversal. Dependency lines are derived views; creating a formula link invokes a host formula-edit command. Store positions in manifest coordinates independent of screen pixels.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-skin-ir/ and DnaTreeCalc/src/dnacalc-shell/src/stage.rs: StageSurface. The source-manifest bead resolves exact intent/projection symbols. Replace tree-first WorkspaceState and the monolithic intent enum.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m2_4 -- --nocapture
cargo xtask accept --id dnacalc-m2.4 --platform windows --require-all
cargo xtask accept --id dnacalc-m2.4 --platform linux --require-all
cargo xtask accept --id dnacalc-m2.4 --platform wasm --require-all
```
- `dnacalc_m2_4__canvas_edit`: Editing a card changes its real worksheet formula and all other skins receive its result delta.
- `dnacalc_m2_4__layout_only`: Moving/resizing 100 cards produces only layout operations and no evaluation calls.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m2.5 — Implement the Excel skin minimum grid and formula bar

**Type:** task · **Priority:** 0 · **Labels:** m2, dnacalc, all

**Blocking prerequisites:** `dnacalc-m2.1`, `cfml-m2.5`.
**Parent epic:** `dnacalc-m2`.

### Outcome
Implement the Excel skin minimum grid and formula bar. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Port pure grid geometry/RenderPlan and one DOM editor; add visible-window subscriptions, formula bar and name box. Render provenance, errors and basic spill outlines. Use host command IDs and shared selection state, not a second grid model.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m2_5 -- --nocapture
cargo xtask accept --id dnacalc-m2.5 --platform windows --require-all
cargo xtask accept --id dnacalc-m2.5 --platform linux --require-all
cargo xtask accept --id dnacalc-m2.5 --platform wasm --require-all
```
- `dnacalc_m2_5__real_grid_edit`: A typed cell entry reaches HostState and redraws only subscribed changed cells.
- `dnacalc_m2_5__far_scroll`: Navigate to XFD1048576 with bounded viewport data rather than a full-sheet DOM or dense cell allocation.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m2.6 — Show formula details and dependency navigation in every skin

**Type:** task · **Priority:** 0 · **Labels:** m2, dnacalc, all

**Blocking prerequisites:** `dnacalc-m2.2`, `dnacalc-m2.3`, `dnacalc-m2.4`, `dnacalc-m2.5`, `cfml-m2.3`.
**Parent epic:** `dnacalc-m2`.

### Outcome
Show formula details and dependency navigation in every skin. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Shared formulas and all four skins over one workbook. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Use shared formula detail panels for source, actual evaluated scalar/error values and direct cell precedents/dependents in all four skins. Preserve typed provenance and diagnostics. This early bead establishes the real shared projection route; named-LAMBDA and realized spill inspection are completed by dnacalc-m3.1 after their engine support lands.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-skin-ir/ and DnaTreeCalc/src/dnacalc-shell/src/stage.rs: StageSurface. The source-manifest bead resolves exact intent/projection symbols. Replace tree-first WorkspaceState and the monolithic intent enum.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m2_6 -- --nocapture
cargo xtask accept --id dnacalc-m2.6 --platform windows --require-all
cargo xtask accept --id dnacalc-m2.6 --platform linux --require-all
cargo xtask accept --id dnacalc-m2.6 --platform wasm --require-all
```
- `dnacalc_m2_6__formula_truth`: The same scalar/reference workbook shows identical formulas, errors, provenance and calculated values in all skins through the actual host projection.
- `dnacalc_m2_6__dependency_jump`: Selecting a precedent navigates to its actual workbook anchor across skins.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m3 — Complete Infrastructure — M3

**Type:** epic · **Priority:** 0 · **Labels:** m3, infra, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for Infrastructure in Workbook-wide references, arrays, names and extension attachment points have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 8 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## infra-m3.1 — Capture and reconcile the complete reference Excel UI inventory

**Type:** task · **Priority:** 0 · **Labels:** m3, infra, oracle

**Blocking prerequisites:** `infra-m1.2`, `chost-m2.2`.
**Parent epic:** `infra-m3`.

### Outcome
Capture and reconcile the complete reference Excel UI inventory. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Pin the reference desktop edition/build, then inventory ribbon/backstage/quick-access/contextual tabs, all dialogs and keyboard commands using public documentation and black-box inspection. Record stable command-family IDs, platform availability and done/partial/not-started status backed by tests. Reconcile every extra entry with a core bead or one separable-area planning bead; never assume the prompt’s list is exhaustive.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-skin-ir/ and DnaTreeCalc/src/dnacalc-shell/src/stage.rs: StageSurface. The source-manifest bead resolves exact intent/projection symbols. Replace tree-first WorkspaceState and the monolithic intent enum.

### Acceptance
```text
cargo xtask accept --id infra-m3.1 --platform windows --require-all
cargo xtask accept --id infra-m3.1 --platform linux --require-all
cargo xtask accept --id infra-m3.1 --platform wasm --require-all
cargo xtask accept --id infra-m3.1 --platform excel --require-all
```
- `infra_m3_1__inventory_reconciles`: Every captured command belongs to one mapped family and every done entry points to an executed interaction/headless test.
- `infra_m3_1__reserved_not_done`: A phase-2 or separable-area placeholder cannot be marked done merely because its button exists.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m3.2 — Prove the unified workbook engine milestone

**Type:** task · **Priority:** 2 · **Labels:** m3, infra, oracle

**Blocking prerequisites:** `ceng-m3.4`, `ceng-m3.5`, `ceng-m3.6`, `ceng-m3.7`, `cfml-m3.4`, `chost-m3.1`, `infra-m1.2`, `infra-m3.3`, `ceng-m3.9`.
**Parent epic:** `infra-m3`.

### Outcome
Prove the unified workbook engine milestone. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Run a retained feature matrix with multi-sheet names, 3D ranges, tables, dynamic references, spill blockers, external snapshots and legacy arrays through HostState. Compare independent captures and assert there is one evaluator route and one graph per workbook.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m3.2 --platform windows --require-all
cargo xtask accept --id infra-m3.2 --platform linux --require-all
cargo xtask accept --id infra-m3.2 --platform wasm --require-all
cargo xtask accept --id infra-m3.2 --platform excel --require-all
```
- `infra_m3_2__workbook_matrix`: Every required feature case is actually calculated and its exact/error/shape verdict retained.
- `infra_m3_2__no_hidden_lane`: Production call instrumentation demonstrates all formulas reached CalcFml, including repeated R1C1 regions.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m3.3 — Complete the advanced skin persistence interchange matrix

**Type:** task · **Priority:** 0 · **Labels:** m3, infra, oracle

**Blocking prerequisites:** `infra-m2.1`, `dnacalc-m3.1`, `cdoc-m3.1`, `cdoc-m3.3`, `chost-m2.4`, `infra-m1.2`.
**Parent epic:** `infra-m3`.

### Outcome
Complete the advanced skin persistence interchange matrix. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Extend the original three-design comparison suite with names that reference other blocks, scoped and renamed aliases, raw/named/invoked LAMBDAs, spill resizing/blocking, metadata stripping, and real-Excel edits. Retain candidate-by-case outcomes and overhead observations; only the selected hybrid ships. Prove a deleted backing sheet produces recoverable orphan diagnostics rather than cached-value resurrection.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m3.3 --platform windows --require-all
cargo xtask accept --id infra-m3.3 --platform linux --require-all
cargo xtask accept --id infra-m3.3 --platform wasm --require-all
cargo xtask accept --id infra-m3.3 --platform excel --require-all
```
- `infra_m3_3__complete_persistence_matrix`: Each of the three candidate designs has a recorded outcome for every case in brief section 3.5; the chosen design passes its declared interchange/recovery behaviour.
- `infra_m3_3__canonical_lambda`: Real-Excel Name Manager edits change the function observed in every DNA skin without a stale duplicate executable definition.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m3.20 — Plan charting and drawing objects implementation

**Type:** task · **Priority:** 2 · **Labels:** m3, infra, tooling

**Blocking prerequisites:** `cdoc-m3.5`, `ceng-m3.8`, `chost-m3.2`.
**Parent epic:** `infra-m3`.

### Outcome
Plan charting and drawing objects implementation. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Produce this area’s executable implementation subgraph, not implementation code: Chart types (including combination/secondary axes and modern families), series evaluation, formatting, themes, labels, legends, shapes/images, anchors, hit-testing and accessibility. Do not implement pivots or printing here. Record scope, non-scope, core dependencies, exact attachment API consumers, late-discovery risks and required Excel captures. Validate and import child beads before marking the area planning task complete. Add those children as blockers of the first-version release gate; this planning bead is not completion of the feature.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh

### Acceptance
```text
cargo xtask accept --id infra-m3.20 --platform windows --require-all
cargo xtask accept --id infra-m3.20 --platform linux --require-all
```
- `infra_m3_20__chart_plan_slots`: Every inventoried chart/drawing command maps to a future implementation bead and the plan names dataset/geometry/format APIs.
- `infra_m3_20__chart_attachment`: Run the existing production dependency-inspector scenario to prove source edits reach chart participants.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Implementing this separable area or deferring any notebook/tree/canvas work. This is the one allowed detailed-planning bead for this area.

---

## infra-m3.21 — Plan pivot tables caches and getpivotdata implementation

**Type:** task · **Priority:** 2 · **Labels:** m3, infra, tooling

**Blocking prerequisites:** `cdoc-m3.5`, `ceng-m3.8`, `chost-m3.2`.
**Parent epic:** `infra-m3`.

### Outcome
Plan pivot tables caches and getpivotdata implementation. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Produce this area’s executable implementation subgraph, not implementation code: Cache records, grouping, calculated fields/items, layout/subtotals/totals, refresh, pivot charts, slicers and GETPIVOTDATA; distinguish worksheet-source and external/OLAP/data-model sources. Do not claim generic cached preservation is refresh. Record scope, non-scope, core dependencies, exact attachment API consumers, late-discovery risks and required Excel captures. Validate and import child beads before marking the area planning task complete. Add those children as blockers of the first-version release gate; this planning bead is not completion of the feature.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh

### Acceptance
```text
cargo xtask accept --id infra-m3.21 --platform windows --require-all
cargo xtask accept --id infra-m3.21 --platform linux --require-all
```
- `infra_m3_21__pivot_plan_slots`: The imported implementation subgraph covers cache, layout, refresh and GETPIVOTDATA with real Excel witnesses.
- `infra_m3_21__pivot_attachment`: The production source-edit scenario dirties the cache participant and rejects false refresh completion.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Implementing this separable area or deferring any notebook/tree/canvas work. This is the one allowed detailed-planning bead for this area.

---

## infra-m3.22 — Plan printing page layout and export implementation

**Type:** task · **Priority:** 2 · **Labels:** m3, infra, tooling

**Blocking prerequisites:** `cdoc-m3.5`, `chost-m3.2`.
**Parent epic:** `infra-m3`.

### Outcome
Plan printing page layout and export implementation. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Produce this area’s executable implementation subgraph, not implementation code: Page setup, margins, paper, orientation, breaks, scaling, repeated titles, headers/footers, print area, charts and font metrics; PDF/export and native/browser print adapters. Do not replace workbook screen layout. Record scope, non-scope, core dependencies, exact attachment API consumers, late-discovery risks and required Excel captures. Validate and import child beads before marking the area planning task complete. Add those children as blockers of the first-version release gate; this planning bead is not completion of the feature.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh

### Acceptance
```text
cargo xtask accept --id infra-m3.22 --platform windows --require-all
cargo xtask accept --id infra-m3.22 --platform linux --require-all
```
- `infra_m3_22__print_plan_slots`: Every Page Layout/print command has an implementation bead and font/geometry risks have measured probes.
- `infra_m3_22__print_attachment`: The production print-snapshot command exports consistent page setup and revision-labelled content.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Implementing this separable area or deferring any notebook/tree/canvas work. This is the one allowed detailed-planning bead for this area.

---

## infra-m3.23 — Plan external data connections and analytical integrations implementation

**Type:** task · **Priority:** 2 · **Labels:** m3, infra, tooling

**Blocking prerequisites:** `ceng-m3.7`, `chost-m3.2`.
**Parent epic:** `infra-m3`.

### Outcome
Plan external data connections and analytical integrations implementation. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Produce this area’s executable implementation subgraph, not implementation code: Import/query/connection refresh, linked data types, CUBE/OLAP and data-model-backed pivot sources; credentials/trust/cancellation/cache provenance; statistical/what-if tools such as Goal Seek and Solver surface inventory. Core external workbook links and TABLE() remain core, not deferred here. Record scope, non-scope, core dependencies, exact attachment API consumers, late-discovery risks and required Excel captures. Validate and import child beads before marking the area planning task complete. Add those children as blockers of the first-version release gate; this planning bead is not completion of the feature.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh

### Acceptance
```text
cargo xtask accept --id infra-m3.23 --platform windows --require-all
cargo xtask accept --id infra-m3.23 --platform linux --require-all
```
- `infra_m3_23__data_plan_slots`: All inventoried external/analytic commands map to a concrete provider/implementation bead or explicitly phase-2 add-in placement.
- `infra_m3_23__external_attachment`: The existing production external-update path carries version/provenance and does not silently fetch network data.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Implementing this separable area or deferring any notebook/tree/canvas work. This is the one allowed detailed-planning bead for this area.

---

## infra-m3.24 — Plan extended desktop command and dialog surface implementation

**Type:** task · **Priority:** 2 · **Labels:** m3, infra, tooling

**Blocking prerequisites:** `chost-m3.2`, `infra-m3.1`.
**Parent epic:** `infra-m3`.

### Outcome
Plan extended desktop command and dialog surface implementation. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Produce this area’s executable implementation subgraph, not implementation code: Reconcile the captured reference Excel edition against the full command inventory: advanced/custom UI, accessibility tools, review/history/sharing, collaboration/service entrypoints and remaining dialogs. Core Format Cells, names, CF/DV, sorting/filtering, protection and formula tools stay in core beads. No inventory gap may be relabelled complete by a disabled button. Record scope, non-scope, core dependencies, exact attachment API consumers, late-discovery risks and required Excel captures. Validate and import child beads before marking the area planning task complete. Add those children as blockers of the first-version release gate; this planning bead is not completion of the feature.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh

### Acceptance
```text
cargo xtask accept --id infra-m3.24 --platform windows --require-all
cargo xtask accept --id infra-m3.24 --platform linux --require-all
```
- `infra_m3_24__ui_tail_plan`: The remaining desktop commands have individually owned implementation beads and explicit platform constraints.
- `infra_m3_24__inventory_no_holes`: The union of core, separable and declared phase-2 inventory entries equals the captured command set, with no missing category.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Implementing this separable area or deferring any notebook/tree/canvas work. This is the one allowed detailed-planning bead for this area.

---

## cfunc-m3 — Complete CalcFunc — M3

**Type:** epic · **Priority:** 0 · **Labels:** m3, cfunc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcFunc in Workbook-wide references, arrays, names and extension attachment points have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 25 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cfunc-m3.1 — Port text and Unicode functions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port text and Unicode functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: CONCAT, CONCATENATE, TEXTJOIN, LEFT, RIGHT, MID, LEN, FIND, SEARCH, REPLACE, SUBSTITUTE, TRIM, CLEAN, UNICODE, UNICHAR, UPPER, LOWER and PROPER. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_1 -- --nocapture
cargo xtask accept --id cfunc-m3.1 --platform windows --require-all
cargo xtask accept --id cfunc-m3.1 --platform linux --require-all
cargo xtask accept --id cfunc-m3.1 --platform wasm --require-all
```
- `cfunc_m3_1__unicode_units`: Surrogate pairs, combining marks, embedded nulls and compatibility-version differences match retained witnesses.
- `cfunc_m3_1__text_errors`: Empty/missing/reference arguments preserve the observed coercions and errors.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.2 — Port numeric rounding and integer functions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port numeric rounding and integer functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: ABS, SIGN, INT, TRUNC, ROUND, ROUNDUP, ROUNDDOWN, MROUND, CEILING, FLOOR, MOD, QUOTIENT, EVEN and ODD. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_2 -- --nocapture
cargo xtask accept --id cfunc-m3.2 --platform windows --require-all
cargo xtask accept --id cfunc-m3.2 --platform linux --require-all
cargo xtask accept --id cfunc-m3.2 --platform wasm --require-all
```
- `cfunc_m3_2__rounding_boundaries`: Positive/negative half cases and signed zero follow the selected mode and domain policy.
- `cfunc_m3_2__large_integers`: Inputs around binary64 integer boundaries do not acquire integer overflow or host-specific casts.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.3 — Port transcendental and trigonometric adapters

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port transcendental and trigonometric adapters. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: EXP, LN, LOG, LOG10, POWER, SQRT, trigonometric and hyperbolic function catalogue entries; call NumericBackend only and do not copy quarantined sequences. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_3 -- --nocapture
cargo xtask accept --id cfunc-m3.3 --platform windows --require-all
cargo xtask accept --id cfunc-m3.3 --platform linux --require-all
cargo xtask accept --id cfunc-m3.3 --platform wasm --require-all
```
- `cfunc_m3_3__backend_called`: A recording numeric backend observes every sensitive operation for this family.
- `cfunc_m3_3__domain_errors`: Zero, negative, overflow and near-domain-boundary inputs retain worksheet error semantics in both modes.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.4 — Port aggregate and counting functions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port aggregate and counting functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: SUM, PRODUCT, SUMSQ, SUMPRODUCT, COUNT, COUNTA, COUNTBLANK, AVERAGE, MIN, MAX and their A-suffixed variants. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_4 -- --nocapture
cargo xtask accept --id cfunc-m3.4 --platform windows --require-all
cargo xtask accept --id cfunc-m3.4 --platform linux --require-all
cargo xtask accept --id cfunc-m3.4 --platform wasm --require-all
```
- `cfunc_m3_4__sparse_blanks`: Whole-column references preserve blank cardinality without materializing every cell.
- `cfunc_m3_4__argument_classes`: Direct logical/text values and values read from ranges follow their distinct rules.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.5 — Port conditional aggregates and criteria matching

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port conditional aggregates and criteria matching. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: SUMIF, SUMIFS, COUNTIF, COUNTIFS, AVERAGEIF, AVERAGEIFS, MAXIFS and MINIFS. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_5 -- --nocapture
cargo xtask accept --id cfunc-m3.5 --platform windows --require-all
cargo xtask accept --id cfunc-m3.5 --platform linux --require-all
cargo xtask accept --id cfunc-m3.5 --platform wasm --require-all
```
- `cfunc_m3_5__criteria_semantics`: Wildcards, escape characters, numeric/text criteria and aligned range sizes match Excel fixtures.
- `cfunc_m3_5__mismatched_shapes`: Unequal criteria ranges return observed errors rather than silently truncate.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.6 — Port lookup and reference-returning functions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port lookup and reference-returning functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: INDEX, MATCH, XMATCH, XLOOKUP, VLOOKUP, HLOOKUP, LOOKUP, CHOOSE, ROW, COLUMN, ROWS, COLUMNS and AREAS. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_6 -- --nocapture
cargo xtask accept --id cfunc-m3.6 --platform windows --require-all
cargo xtask accept --id cfunc-m3.6 --platform linux --require-all
cargo xtask accept --id cfunc-m3.6 --platform wasm --require-all
```
- `cfunc_m3_6__reference_result`: Reference-returning INDEX remains a reference until the consuming context dereferences it.
- `cfunc_m3_6__lookup_edges`: Duplicate keys, search direction, approximate boundaries and not-found defaults match witnesses.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.7 — Port date and time functions

**Type:** task · **Priority:** 1 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port date and time functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: DATE, TIME, YEAR, MONTH, DAY, HOUR, MINUTE, SECOND, DATEVALUE, TIMEVALUE, DAYS, DAYS360, EDATE and EOMONTH. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_7 -- --nocapture
cargo xtask accept --id cfunc-m3.7 --platform windows --require-all
cargo xtask accept --id cfunc-m3.7 --platform linux --require-all
cargo xtask accept --id cfunc-m3.7 --platform wasm --require-all
```
- `cfunc_m3_7__date_systems`: 1900 leap-year compatibility and 1904 offsets use the supplied workbook date system.
- `cfunc_m3_7__locale_parse`: Date/time text parsing uses the supplied locale and never the process locale.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.8 — Port business-calendar functions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port business-calendar functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: WEEKDAY, WEEKNUM, ISOWEEKNUM, NETWORKDAYS, NETWORKDAYS.INTL, WORKDAY, WORKDAY.INTL and YEARFRAC. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_8 -- --nocapture
cargo xtask accept --id cfunc-m3.8 --platform windows --require-all
cargo xtask accept --id cfunc-m3.8 --platform linux --require-all
cargo xtask accept --id cfunc-m3.8 --platform wasm --require-all
```
- `cfunc_m3_8__calendar_edges`: Weekend masks, leap boundaries and duplicate holidays match oracle cases.
- `cfunc_m3_8__invalid_calendar`: Invalid weekend codes and date bounds produce the captured errors.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.9 — Port distribution functions in bounded dispatch batches

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port distribution functions in bounded dispatch batches. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: Normal, lognormal, beta, gamma, chi-square, Student t, F, binomial, negative-binomial, Poisson, Weibull, exponential and hypergeometric catalogue entries; copy semantics/tests unchanged except backend plumbing. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_9 -- --nocapture
cargo xtask accept --id cfunc-m3.9 --platform windows --require-all
cargo xtask accept --id cfunc-m3.9 --platform linux --require-all
cargo xtask accept --id cfunc-m3.9 --platform wasm --require-all
```
- `cfunc_m3_9__tails_and_inverse`: Interior, tail and inverse-roundtrip witnesses are preserved with mode-labelled numerical verdicts.
- `cfunc_m3_9__distribution_domains`: Invalid probabilities and shape parameters return exact worksheet error types.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.10 — Port descriptive statistics and ranking functions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port descriptive statistics and ranking functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: MEDIAN, MODE, percentile/quartile families, RANK, LARGE, SMALL, variance/stdev families, skew/kurtosis and geometric/harmonic means. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_10 -- --nocapture
cargo xtask accept --id cfunc-m3.10 --platform windows --require-all
cargo xtask accept --id cfunc-m3.10 --platform linux --require-all
cargo xtask accept --id cfunc-m3.10 --platform wasm --require-all
```
- `cfunc_m3_10__statistical_coercion`: Empty, text, error and logical array inputs match the established coercion matrix.
- `cfunc_m3_10__sample_edges`: Zero/one-element and tied-rank cases match independent expectations.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.11 — Port regression correlation and forecast functions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port regression correlation and forecast functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: CORREL, PEARSON, COVARIANCE, SLOPE, INTERCEPT, RSQ, STEYX, LINEST, LOGEST, TREND, GROWTH, FORECAST and ETS entries. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_11 -- --nocapture
cargo xtask accept --id cfunc-m3.11 --platform windows --require-all
cargo xtask accept --id cfunc-m3.11 --platform linux --require-all
cargo xtask accept --id cfunc-m3.11 --platform wasm --require-all
```
- `cfunc_m3_11__regression_shapes`: Multi-variable and constant-column cases produce captured array shapes and values.
- `cfunc_m3_11__ill_conditioned`: Singular/ill-conditioned witnesses are retained and classified rather than tolerance-laundered.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.12 — Port financial cash-flow functions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port financial cash-flow functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: PV, FV, PMT, NPER, RATE, IPMT, PPMT, CUMIPMT, CUMPRINC, NPV, IRR, MIRR, XNPV, XIRR and RRI. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_12 -- --nocapture
cargo xtask accept --id cfunc-m3.12 --platform windows --require-all
cargo xtask accept --id cfunc-m3.12 --platform linux --require-all
cargo xtask accept --id cfunc-m3.12 --platform wasm --require-all
```
- `cfunc_m3_12__solver_branches`: Multiple-root, no-root and near-zero-rate cases match captured convergence/error behaviour.
- `cfunc_m3_12__cashflow_dates`: Unsorted and repeated dates use the correct documented/captured validation order.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.13 — Port securities and depreciation functions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port securities and depreciation functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: Bond price/yield/duration, coupon-date/day families, accrued interest, discount, treasury bills, SLN, SYD, DB, DDB, VDB and AMOR entries. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_13 -- --nocapture
cargo xtask accept --id cfunc-m3.13 --platform windows --require-all
cargo xtask accept --id cfunc-m3.13 --platform linux --require-all
cargo xtask accept --id cfunc-m3.13 --platform wasm --require-all
```
- `cfunc_m3_13__basis_dates`: All admitted day-count bases and coupon boundaries have mode-labelled witnesses.
- `cfunc_m3_13__depreciation_edges`: Partial periods and invalid cost/salvage/life inputs preserve expected errors.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.14 — Port complex and engineering functions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`, `cfunc-m3.23`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port complex and engineering functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: Complex IM families, CONVERT, BESSEL, ERF/ERFC, DELTA, GESTEP and base-conversion/bitwise functions. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_14 -- --nocapture
cargo xtask accept --id cfunc-m3.14 --platform windows --require-all
cargo xtask accept --id cfunc-m3.14 --platform linux --require-all
cargo xtask accept --id cfunc-m3.14 --platform wasm --require-all
```
- `cfunc_m3_14__complex_branches`: Complex signs, suffixes and branch-cut witnesses match the declared numeric target.
- `cfunc_m3_14__conversion_bounds`: Bit widths, base ranges and unit incompatibilities are validated without host-integer overflow.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.15 — Port database and filtering aggregates

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port database and filtering aggregates. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: D-suffixed database functions, SUBTOTAL and AGGREGATE. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_15 -- --nocapture
cargo xtask accept --id cfunc-m3.15 --platform windows --require-all
cargo xtask accept --id cfunc-m3.15 --platform linux --require-all
cargo xtask accept --id cfunc-m3.15 --platform wasm --require-all
```
- `cfunc_m3_15__visibility_dependencies`: Filtered versus manually hidden rows yield the correct values and dependency reads.
- `cfunc_m3_15__database_criteria`: Repeated field criteria and text wildcard cases retain the observed database semantics.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.16 — Port dynamic array transformation functions

**Type:** task · **Priority:** 1 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port dynamic array transformation functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: FILTER, SORT, SORTBY, UNIQUE, SEQUENCE, RANDARRAY, TRANSPOSE, TAKE, DROP, CHOOSECOLS, CHOOSEROWS, HSTACK, VSTACK, TOCOL, TOROW, WRAPROWS, WRAPCOLS and EXPAND. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_16 -- --nocapture
cargo xtask accept --id cfunc-m3.16 --platform windows --require-all
cargo xtask accept --id cfunc-m3.16 --platform linux --require-all
cargo xtask accept --id cfunc-m3.16 --platform wasm --require-all
```
- `cfunc_m3_16__array_shapes`: Empty, zero-length, padding, ordering and error cases preserve Excel shapes and errors.
- `cfunc_m3_16__lazy_source`: Sparse/reference sources are consumed through the typed reader rather than eagerly copied by an adapter.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.17 — Port callable array helpers

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`, `cfml-m2.3`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port callable array helpers. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: MAP, REDUCE, SCAN, BYROW, BYCOL, MAKEARRAY and ISOMITTED. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_17 -- --nocapture
cargo xtask accept --id cfunc-m3.17 --platform windows --require-all
cargo xtask accept --id cfunc-m3.17 --platform linux --require-all
cargo xtask accept --id cfunc-m3.17 --platform wasm --require-all
```
- `cfunc_m3_17__callback_contract`: Invocation count, argument shapes and callback error order match observed helper semantics.
- `cfunc_m3_17__nested_callables`: Nested helper lambdas do not share mutable frames or leak captures across evaluations.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.18 — Port information and host-context functions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port information and host-context functions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: IS families, TYPE, N, NA, ERROR.TYPE, CELL, INFO, SHEET, SHEETS and FORMULATEXT. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_18 -- --nocapture
cargo xtask accept --id cfunc-m3.18 --platform windows --require-all
cargo xtask accept --id cfunc-m3.18 --platform linux --require-all
cargo xtask accept --id cfunc-m3.18 --platform wasm --require-all
```
- `cfunc_m3_18__host_facts`: Each host-sensitive result comes from a declared typed provider dependency.
- `cfunc_m3_18__unavailable_facts`: Missing platform facts return a typed unavailable result rather than an invented computer/environment value.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.19 — Port volatile and provider-backed function adapters

**Type:** task · **Priority:** 1 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port volatile and provider-backed function adapters. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: NOW, TODAY, RAND, RANDBETWEEN, RTD, external/CUBE/link/data/image/catalogue adapters already present in the pinned source. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_19 -- --nocapture
cargo xtask accept --id cfunc-m3.19 --platform windows --require-all
cargo xtask accept --id cfunc-m3.19 --platform linux --require-all
cargo xtask accept --id cfunc-m3.19 --platform wasm --require-all
```
- `cfunc_m3_19__replay_providers`: Fixed time/random/external tapes reproduce the same outputs and read facts across targets.
- `cfunc_m3_19__missing_provider`: Unavailable services are reported and counted, never answered from file-cache values.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.20 — Port modern text and locale-sensitive catalogue additions

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port modern text and locale-sensitive catalogue additions. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: TEXTSPLIT, TEXTBEFORE, TEXTAFTER, regex entries and all locale/version-sensitive text additions present in the pinned catalogue. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_20 -- --nocapture
cargo xtask accept --id cfunc-m3.20 --platform windows --require-all
cargo xtask accept --id cfunc-m3.20 --platform linux --require-all
cargo xtask accept --id cfunc-m3.20 --platform wasm --require-all
```
- `cfunc_m3_20__compatibility_version`: The same witnessed input is evaluated under each admitted compatibility version with recorded differences.
- `cfunc_m3_20__catalogue_presence`: Entries absent from the pinned catalogue are reported as absent rather than fabricated from a later Excel version.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.21 — Port remaining logical and compatibility aliases

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.2`, `infra-m0.4`.
**Parent epic:** `cfunc-m3`.

### Outcome
Port remaining logical and compatibility aliases. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Port the named family only: AND, OR, XOR, NOT, TRUE, FALSE, IFNA and all legacy/compatibility aliases in the pinned catalogue; lazy scheduling remains owned by CalcFml metadata. Preserve the existing function tests and arithmetic order unless an independent witness requires a change. Route metadata, modes, providers and dispatch through the single catalogue. Produce a per-ID manifest of implemented, divergent and unsupported cases; no blanket parity claim.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: FunctionCallTarget. Resolve each entry to its exact implementation/test path in the pinned source manifest before copying. Port only this named family and its transitive helpers; do not copy other families.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_21 -- --nocapture
cargo xtask accept --id cfunc-m3.21 --platform windows --require-all
cargo xtask accept --id cfunc-m3.21 --platform linux --require-all
cargo xtask accept --id cfunc-m3.21 --platform wasm --require-all
```
- `cfunc_m3_21__alias_identity`: Aliases resolve to the canonical declaration and share help, arity and invocation semantics.
- `cfunc_m3_21__logical_arguments`: Array/reference/direct arguments obey the same witnessed coercion order as Excel.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.22 — Declare numerical sensitivity and generate backend routing

**Type:** task · **Priority:** 1 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m1.1`.
**Parent epic:** `cfunc-m3`.

### Outcome
Declare numerical sensitivity and generate backend routing. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Add metadata for invariant, arithmetic-order, transcendental, reduction and numeric-text sensitivity. Generate resolved per-mode call handles from it, and derive sensitivity through helper dependencies. Forbid ad-hoc function-name/mode branches in CalcFml or CalcEngine.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/function.rs: FunctionMeta; crates/oxfunc_core/src/function_call.rs: FunctionCallTarget, FunctionCallScratch, FunctionExecutionContextBundle; crates/oxfunc_core/src/registry.rs; crates/oxfunc_core/src/functions/surface_dispatch.rs. Paths after the first are relative to OxFunc/.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_22 -- --nocapture
cargo xtask accept --id cfunc-m3.22 --platform windows --require-all
cargo xtask accept --id cfunc-m3.22 --platform linux --require-all
cargo xtask accept --id cfunc-m3.22 --platform wasm --require-all
```
- `cfunc_m3_22__sensitivity_closure`: Every admitted function/operator has one explicit numeric policy and helper dependencies cannot hide a sensitive call.
- `cfunc_m3_22__mode_routes`: A recording backend demonstrates generated dispatch selects the requested mode without a string switch.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.23 — Implement the deterministic portable Fast backend

**Type:** task · **Priority:** 0 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m3.22`.
**Parent epic:** `cfunc-m3`.

### Outcome
Implement the deterministic portable Fast backend. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Pin a portable Rust math implementation and compiler flags; forbid fast-math and uncontrolled FMA contraction. Standardize rounding/subnormal/error handling and use deterministic reduction order. Label Fast distinct from Exact; make unsupported Exact an explicit error rather than a fallback.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs: exp, pow_direct, atan and the fyl2x_fn expansion are QUARANTINED, not port sources until provenance clearance. OxFunc/crates/oxfunc_core/src/excel_numeric/ is a discovery location only. Use independent public-ISA/black-box witnesses for replacement code.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_23 -- --nocapture
cargo xtask accept --id cfunc-m3.23 --platform windows --require-all
cargo xtask accept --id cfunc-m3.23 --platform linux --require-all
cargo xtask accept --id cfunc-m3.23 --platform wasm --require-all
```
- `cfunc_m3_23__cross_target_bits`: Recorded Fast vectors have identical output bits on Windows, Linux and wasm under the pinned implementation.
- `cfunc_m3_23__no_exact_fallback`: Requesting an unavailable Exact target fails before calculation begins.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.24 — Implement a reviewed native Exact instruction adapter

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, native

**Blocking prerequisites:** `cfunc-m3.22`, `infra-m0.5`.
**Parent epic:** `cfunc-m3`.

### Outcome
Implement a reviewed native Exact instruction adapter. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
After provenance clearance, implement the minimal x87 wrapper from public ISA documentation and independently derived black-box arithmetic graphs, in a dedicated cfg-gated backend crate. Restore control/status policy correctly and contain unsafe code. Select only a recorded Excel-build/CPU numerical target; do not port unresolved NT-derived material.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs: exp, pow_direct, atan and the fyl2x_fn expansion are QUARANTINED, not port sources until provenance clearance. OxFunc/crates/oxfunc_core/src/excel_numeric/ is a discovery location only. Use independent public-ISA/black-box witnesses for replacement code.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_24 -- --nocapture
cargo xtask accept --id cfunc-m3.24 --platform windows --require-all
cargo xtask accept --id cfunc-m3.24 --platform linux --require-all
cargo xtask accept --id cfunc-m3.24 --platform wasm --require-all
```
- `cfunc_m3_24__native_witness_bits`: All admitted independent Exact witnesses match the selected target bit-for-bit.
- `cfunc_m3_24__fpu_environment`: Calls preserve the caller floating-point control environment and repeated calls do not leak x87 stack state.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux x86-64 for the native Exact backend. On wasm (and other architectures) exercise NumericModeUnavailable and retained-vector replay; do not silently select Fast.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m3.25 — Audit complete catalogue migration and generated outputs

**Type:** task · **Priority:** 2 · **Labels:** m3, cfunc, all

**Blocking prerequisites:** `cfunc-m3.1`, `cfunc-m3.2`, `cfunc-m3.3`, `cfunc-m3.4`, `cfunc-m3.5`, `cfunc-m3.6`, `cfunc-m3.7`, `cfunc-m3.8`, `cfunc-m3.9`, `cfunc-m3.10`, `cfunc-m3.11`, `cfunc-m3.12`, `cfunc-m3.13`, `cfunc-m3.14`, `cfunc-m3.15`, `cfunc-m3.16`, `cfunc-m3.17`, `cfunc-m3.18`, `cfunc-m3.19`, `cfunc-m3.20`, `cfunc-m3.21`, `cfunc-m3.22`, `cfunc-m3.23`, `cfunc-m3.24`.
**Parent epic:** `cfunc-m3`.

### Outcome
Audit complete catalogue migration and generated outputs. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Compare the pinned old catalogue with the new declaration set, including operators, aliases and hidden/internal worksheet surfaces. Require a one-to-one disposition for every ID and no missing implemented function. Generate help/ExcelFunctionsHandbook/XLL-tooling tables from the same declaration; report unverified/divergent functions honestly.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/function.rs: FunctionMeta; crates/oxfunc_core/src/function_call.rs: FunctionCallTarget, FunctionCallScratch, FunctionExecutionContextBundle; crates/oxfunc_core/src/registry.rs; crates/oxfunc_core/src/functions/surface_dispatch.rs. Paths after the first are relative to OxFunc/.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m3_25 -- --nocapture
cargo xtask accept --id cfunc-m3.25 --platform windows --require-all
cargo xtask accept --id cfunc-m3.25 --platform linux --require-all
cargo xtask accept --id cfunc-m3.25 --platform wasm --require-all
```
- `cfunc_m3_25__catalogue_bijection`: Every source ID maps to an implementation or a named explicit unsupported disposition; no silent omission exists.
- `cfunc_m3_25__derived_paths`: Changing one declaration regenerates matching dispatch, metadata, help and verification tables.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m3 — Complete CalcFml — M3

**Type:** epic · **Priority:** 1 · **Labels:** m3, cfml, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcFml in Workbook-wide references, arrays, names and extension attachment points have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 5 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cfml-m3.1 — Bind scoped and caller-relative defined names

**Type:** task · **Priority:** 1 · **Labels:** m3, cfml, all

**Blocking prerequisites:** `cfml-m2.2`.
**Parent epic:** `cfml-m3`.

### Outcome
Bind scoped and caller-relative defined names. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Resolve workbook and sheet-local names, shadowing, forward names and relative name formulas against an explicit invocation anchor. Share name definition plans but key evaluated name instances by caller context when required. Keep callable names and reference-valued names typed.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/reference_engine.rs: StrictExcelGridReferenceProfile, ExcelGridReferenceSystemProvider; OxFml/crates/oxfml_core/src/binding/profile.rs: reference requests/normal-form identity. Extract Excel semantics, not profiles or BTreeMap<String,...> providers.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m3_1 -- --nocapture
cargo xtask accept --id cfml-m3.1 --platform windows --require-all
cargo xtask accept --id cfml-m3.1 --platform linux --require-all
cargo xtask accept --id cfml-m3.1 --platform wasm --require-all
```
- `cfml_m3_1__name_shadowing`: Sheet-local names shadow workbook names without cross-sheet cache aliasing.
- `cfml_m3_1__relative_name`: One relative name invoked from two cells reads different intended references and recalculates after a definition edit.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m3.2 — Bind structured tables and 3D sheet spans

**Type:** task · **Priority:** 1 · **Labels:** m3, cfml, all

**Blocking prerequisites:** `cfml-m3.1`.
**Parent epic:** `cfml-m3`.

### Outcome
Bind structured tables and 3D sheet spans. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Compile table sections, escaped column labels, row-context references and 3D sheet spans to IDs and relative operands. Sheet-span membership is supplied from current sheet order, not frozen during parse. Preserve unsupported function/reference combinations as Excel errors or typed capability gaps.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/reference_engine.rs: StrictExcelGridReferenceProfile, ExcelGridReferenceSystemProvider; OxFml/crates/oxfml_core/src/binding/profile.rs: reference requests/normal-form identity. Extract Excel semantics, not profiles or BTreeMap<String,...> providers.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m3_2 -- --nocapture
cargo xtask accept --id cfml-m3.2 --platform windows --require-all
cargo xtask accept --id cfml-m3.2 --platform linux --require-all
cargo xtask accept --id cfml-m3.2 --platform wasm --require-all
```
- `cfml_m3_2__table_context`: This-row and headers/totals references resolve correctly in and outside a table.
- `cfml_m3_2__span_lifecycle`: Moving/adding/deleting a sheet changes 3D membership exactly as the recorded structural witness requires.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m3.3 — Report dynamic references and reference algebra reads

**Type:** task · **Priority:** 1 · **Labels:** m3, cfml, all

**Blocking prerequisites:** `cfml-m3.2`.
**Parent epic:** `cfml-m3`.

### Outcome
Report dynamic references and reference algebra reads. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Implement range, union, intersection, spill and implicit-intersection operators; route INDIRECT/OFFSET text resolution through CalcFml’s Excel binder. Return dynamic read footprints, including failed lookup/absence watches, without owning the graph.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/reference_engine.rs: StrictExcelGridReferenceProfile, ExcelGridReferenceSystemProvider; OxFml/crates/oxfml_core/src/binding/profile.rs: reference requests/normal-form identity. Extract Excel semantics, not profiles or BTreeMap<String,...> providers.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m3_3 -- --nocapture
cargo xtask accept --id cfml-m3.3 --platform windows --require-all
cargo xtask accept --id cfml-m3.3 --platform linux --require-all
cargo xtask accept --id cfml-m3.3 --platform wasm --require-all
```
- `cfml_m3_3__dynamic_switch`: Changing INDIRECT text produces a replacement read footprint and the correct value.
- `cfml_m3_3__absent_target`: A currently invalid name/sheet reference records enough identity dependency to become valid after the missing target is created.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m3.4 — Preserve array evaluation and legacy array contexts

**Type:** task · **Priority:** 1 · **Labels:** m3, cfml, all

**Blocking prerequisites:** `cfml-m2.4`, `cfunc-m3.16`.
**Parent epic:** `cfml-m3`.

### Outcome
Preserve array evaluation and legacy array contexts. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Implement scalar, dynamic-array and legacy-CSE evaluation contexts, broadcasting, omitted arguments, nested arrays and array-valued errors. Keep cell publication separate from callable/raw formula values and declare allocation limits as observable resource outcomes.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/eval/mod.rs: compiled expressions and evaluation; OxFml/crates/oxfml_core/src/binding/profile.rs: ReferenceNormalFormKey, ReferenceFingerprintPolicy. Discard ReferencePolicy, host-reference syntax and the tree profile.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m3_4 -- --nocapture
cargo xtask accept --id cfml-m3.4 --platform windows --require-all
cargo xtask accept --id cfml-m3.4 --platform linux --require-all
cargo xtask accept --id cfml-m3.4 --platform wasm --require-all
```
- `cfml_m3_4__legacy_cse`: The same expression in legacy array and dynamic-array contexts matches captured shape/intersection differences.
- `cfml_m3_4__array_limits`: A resource limit is typed and counted, never a materialization cap returning a cached answer.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m3.5 — Bind external workbook references through a typed catalogue

**Type:** task · **Priority:** 1 · **Labels:** m3, cfml, all

**Blocking prerequisites:** `cfml-m3.2`.
**Parent epic:** `cfml-m3`.

### Outcome
Bind external workbook references through a typed catalogue. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Add external book IDs, link versions and open/closed book resolution without filesystem/network access in CalcFml. Return requested external reference facts for the provider. Preserve workbook/sheet name quoting and canonical file formula serialization.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/reference_engine.rs: StrictExcelGridReferenceProfile, ExcelGridReferenceSystemProvider; OxFml/crates/oxfml_core/src/binding/profile.rs: reference requests/normal-form identity. Extract Excel semantics, not profiles or BTreeMap<String,...> providers.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m3_5 -- --nocapture
cargo xtask accept --id cfml-m3.5 --platform windows --require-all
cargo xtask accept --id cfml-m3.5 --platform linux --require-all
cargo xtask accept --id cfml-m3.5 --platform wasm --require-all
```
- `cfml_m3_5__external_identity`: Two links to similarly named workbooks remain distinct by link identity.
- `cfml_m3_5__unresolved_link`: An unavailable external target produces a typed missing dependency with no silent use of an old file cache.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m3 — Complete CalcEngine — M3

**Type:** epic · **Priority:** 1 · **Labels:** m3, ceng, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcEngine in Workbook-wide references, arrays, names and extension attachment points have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 9 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## ceng-m3.1 — Index ranges whole axes and absence dependencies

**Type:** task · **Priority:** 1 · **Labels:** m3, ceng, all

**Blocking prerequisites:** `ceng-m2.1`, `cfml-m3.3`.
**Parent epic:** `ceng-m3`.

### Outcome
Index ranges whole axes and absence dependencies. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Implement compressed rectangle and whole-row/column reverse dependency indices over interned IDs. Include value, shape, existence, visibility, name and metadata facets. Use point/range intersection queries and sparse cursors; do not expand whole axes into per-cell edges.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridAxisValueDependency, GridAxisVisibilityDependency, GridSheetSpanDependency and reverse range indexing. Reimplement with integer IDs in one workbook-wide graph; do not inherit the old per-sheet scheduler.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m3_1 -- --nocapture
cargo xtask accept --id ceng-m3.1 --platform windows --require-all
cargo xtask accept --id ceng-m3.1 --platform linux --require-all
cargo xtask accept --id ceng-m3.1 --platform wasm --require-all
```
- `ceng_m3_1__whole_column`: Editing a populated or newly created cell in a whole-column dependency dirties the right dependents.
- `ceng_m3_1__compressed_storage`: The number of stored dependency records does not grow with the number of blank cells in a referenced whole sheet.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m3.2 — Schedule scoped name instances and forward lookups

**Type:** task · **Priority:** 1 · **Labels:** m3, ceng, all

**Blocking prerequisites:** `ceng-m3.1`, `cfml-m3.1`.
**Parent epic:** `ceng-m3`.

### Outcome
Schedule scoped name instances and forward lookups. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Represent names as definition/lookup nodes plus caller-specific instances when relative. Track definition edits, rename and undefined-to-defined transitions. Build cell/name cycles in the same graph, not a separate name evaluator.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridAxisValueDependency, GridAxisVisibilityDependency, GridSheetSpanDependency and reverse range indexing. Reimplement with integer IDs in one workbook-wide graph; do not inherit the old per-sheet scheduler.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m3_2 -- --nocapture
cargo xtask accept --id ceng-m3.2 --platform windows --require-all
cargo xtask accept --id ceng-m3.2 --platform linux --require-all
cargo xtask accept --id ceng-m3.2 --platform wasm --require-all
```
- `ceng_m3_2__relative_names`: Two callers of one relative name calculate independently and invalidate on their actual inputs.
- `ceng_m3_2__name_cycle`: Cell-name-name cycles are detected as one workbook SCC, not an arbitrary recursion timeout.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m3.3 — Update dynamic read overlays atomically

**Type:** task · **Priority:** 1 · **Labels:** m3, ceng, all

**Blocking prerequisites:** `ceng-m3.1`, `cfml-m3.3`.
**Parent epic:** `ceng-m3`.

### Outcome
Update dynamic read overlays atomically. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Commit observed read footprints only with their evaluation result. Replace obsolete dynamic edges; retain conservative watches for unresolved lookups. If new reads expose an uncalculated prerequisite or a cycle, reschedule/reclassify before publication and report the reason.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridAxisValueDependency, GridAxisVisibilityDependency, GridSheetSpanDependency and reverse range indexing. Reimplement with integer IDs in one workbook-wide graph; do not inherit the old per-sheet scheduler.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m3_3 -- --nocapture
cargo xtask accept --id ceng-m3.3 --platform windows --require-all
cargo xtask accept --id ceng-m3.3 --platform linux --require-all
cargo xtask accept --id ceng-m3.3 --platform wasm --require-all
```
- `ceng_m3_3__switch_removes_edge`: After INDIRECT changes targets, edits to the old target no longer dirty the dependent.
- `ceng_m3_3__late_prerequisite`: An evaluation that discovers a dirty prerequisite cannot publish a value based on stale input.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m3.4 — Implement spill allocation and blocker invalidation

**Type:** task · **Priority:** 1 · **Labels:** m3, ceng, all

**Blocking prerequisites:** `ceng-m3.3`, `cfml-m3.4`.
**Parent epic:** `ceng-m3`.

### Outcome
Implement spill allocation and blocker invalidation. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Port spill ownership/extent/blocker concepts into a transactional ledger. Detect occupied cells, merged cells, table conflicts, sheet boundaries and competing spills; publish anchor errors and remove obsolete children atomically. Index blockers even for failed spills.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/spill_ledger.rs (spill ownership/blocker design; resolve symbols at source pin); OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridSpillDependency, GridSpillBlockerDependency.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m3_4 -- --nocapture
cargo xtask accept --id ceng-m3.4 --platform windows --require-all
cargo xtask accept --id ceng-m3.4 --platform linux --require-all
cargo xtask accept --id ceng-m3.4 --platform wasm --require-all
```
- `ceng_m3_4__unblock_spill`: Clearing a blocker recalculates the anchor and publishes the complete new extent.
- `ceng_m3_4__shrink_overlap`: Shrinking or competing spills never leave orphan children or overwrite authored cells.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m3.5 — Calculate sheet spans and table-structure dependencies

**Type:** task · **Priority:** 1 · **Labels:** m3, ceng, all

**Blocking prerequisites:** `ceng-m3.2`, `cfml-m3.2`.
**Parent epic:** `ceng-m3`.

### Outcome
Calculate sheet spans and table-structure dependencies. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Add table membership/column/header/totals nodes and sheet-order/span nodes to the workbook graph. Expand values lazily in current order; invalidate on schema changes even when numeric cells are unchanged. Maintain row-visibility facets for SUBTOTAL/AGGREGATE.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridAxisValueDependency, GridAxisVisibilityDependency, GridSheetSpanDependency and reverse range indexing. Reimplement with integer IDs in one workbook-wide graph; do not inherit the old per-sheet scheduler.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m3_5 -- --nocapture
cargo xtask accept --id ceng-m3.5 --platform windows --require-all
cargo xtask accept --id ceng-m3.5 --platform linux --require-all
cargo xtask accept --id ceng-m3.5 --platform wasm --require-all
```
- `ceng_m3_5__span_move`: Reordering sheets changes a 3D result without rebuilding independent sheets.
- `ceng_m3_5__table_resize`: Adding a table row or renaming a column updates dependent formulas and preserves stable table IDs.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m3.6 — Replay volatile time and random effects per tick

**Type:** task · **Priority:** 1 · **Labels:** m3, ceng, all

**Blocking prerequisites:** `ceng-m3.3`, `cfunc-m3.19`.
**Parent epic:** `ceng-m3`.

### Outcome
Replay volatile time and random effects per tick. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Provide explicit tick/time/random effect tapes, node-stable RNG streams and invocation order. Distinguish volatility invalidation from external/RTD updates. Permit oracle capture injection without claiming the deterministic local RNG reproduces Excel’s random generator.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m3_6 -- --nocapture
cargo xtask accept --id ceng-m3.6 --platform windows --require-all
cargo xtask accept --id ceng-m3.6 --platform linux --require-all
cargo xtask accept --id ceng-m3.6 --platform wasm --require-all
```
- `ceng_m3_6__tick_replay`: The same snapshot and effect tape produce identical values on all targets.
- `ceng_m3_6__volatile_scope`: A volatile function recalculates under the requested trigger while an RTD-only change follows its own affected nodes.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m3.7 — Resolve external workbook snapshots and link updates

**Type:** task · **Priority:** 1 · **Labels:** m3, ceng, all

**Blocking prerequisites:** `ceng-m3.3`, `cfml-m3.5`.
**Parent epic:** `ceng-m3`.

### Outcome
Resolve external workbook snapshots and link updates. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Use versioned external input snapshots supplied by the host, with an explicit trust/provenance policy. Support linked open workbooks through host-coordinated operations while each workbook retains one graph; record unavailable/closed-link cached observations distinctly from calculated values.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m3_7 -- --nocapture
cargo xtask accept --id ceng-m3.7 --platform windows --require-all
cargo xtask accept --id ceng-m3.7 --platform linux --require-all
cargo xtask accept --id ceng-m3.7 --platform wasm --require-all
```
- `ceng_m3_7__link_update`: A versioned external value update invalidates only its dependent graph region.
- `ceng_m3_7__stale_external`: Results that depend on untrusted stale external observations cannot receive clean Calculated provenance.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m3.8 — Attach generic derived participants to the workbook graph

**Type:** task · **Priority:** 1 · **Labels:** m3, ceng, all

**Blocking prerequisites:** `ceng-m3.5`, `cdoc-m3.5`.
**Parent epic:** `ceng-m3`.

### Outcome
Attach generic derived participants to the workbook graph. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Introduce sealed production participant kinds for chart-series reads, pivot source/cache revisions and layout-derived reads using one graph. Expose invalidation/evaluation hooks as data, not a second scheduler. A production dependency inspector and host object registry consume the hooks now.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridAxisValueDependency, GridAxisVisibilityDependency, GridSheetSpanDependency and reverse range indexing. Reimplement with integer IDs in one workbook-wide graph; do not inherit the old per-sheet scheduler.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m3_8 -- --nocapture
cargo xtask accept --id ceng-m3.8 --platform windows --require-all
cargo xtask accept --id ceng-m3.8 --platform linux --require-all
cargo xtask accept --id ceng-m3.8 --platform wasm --require-all
```
- `ceng_m3_8__series_invalidates`: Editing a referenced cell marks the actual chart-series participant dirty.
- `ceng_m3_8__pivot_source_invalidates`: A table membership change invalidates a pivot-source participant without pretending the pivot has been refreshed.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m3.9 — Unify connected open workbooks in one calculation domain

**Type:** task · **Priority:** 2 · **Labels:** m3, ceng, all

**Blocking prerequisites:** `ceng-m3.7`.
**Parent epic:** `ceng-m3`.

### Outcome
Unify connected open workbooks in one calculation domain. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Allow one EngineState calculation domain to index connected open workbooks by WorkbookId, using a revision vector and the same graph/scheduler. External closed snapshots remain provider leaves. Do not introduce inter-sheet or inter-workbook fixed-round federation.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m3_9 -- --nocapture
cargo xtask accept --id ceng-m3.9 --platform windows --require-all
cargo xtask accept --id ceng-m3.9 --platform linux --require-all
cargo xtask accept --id ceng-m3.9 --platform wasm --require-all
```
- `ceng_m3_9__open_book_chain`: A dependent chain spanning two open workbooks is topologically scheduled in the same graph.
- `ceng_m3_9__book_identity`: Two workbooks with identically named sheets and cells cannot alias nodes, names or cache entries.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m3 — Complete CalcDoc — M3

**Type:** epic · **Priority:** 1 · **Labels:** m3, cdoc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcDoc in Workbook-wide references, arrays, names and extension attachment points have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 6 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cdoc-m3.1 — Read and write workbook and sheet defined names

**Type:** task · **Priority:** 1 · **Labels:** m3, cdoc, all

**Blocking prerequisites:** `cdoc-m2.1`.
**Parent epic:** `cdoc-m3`.

### Outcome
Read and write workbook and sheet defined names. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Implement scoped defined names, relative formulas, hidden/internal names and built-in print/filter names with stable local IDs. Preserve source spelling and formula text, and author additions/deletions without mixing manifest locator names with user aliases.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m3_1 -- --nocapture
cargo xtask accept --id cdoc-m3.1 --platform windows --require-all
cargo xtask accept --id cdoc-m3.1 --platform linux --require-all
cargo xtask accept --id cdoc-m3.1 --platform wasm --require-all
```
- `cdoc_m3_1__scoped_roundtrip`: Workbook and sheet-local duplicate name spellings survive independently.
- `cdoc_m3_1__name_authoring`: Adding a named LAMBDA and deleting a normal name yields repair-free workbook XML.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m3.2 — Read and write structured table definitions

**Type:** task · **Priority:** 1 · **Labels:** m3, cdoc, all

**Blocking prerequisites:** `cdoc-m3.1`.
**Parent epic:** `cdoc-m3`.

### Outcome
Read and write structured table definitions. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Model table IDs, display names, ranges, columns, calculated/totals formulas, filters and style data; maintain relationships and table parts on create/resize/delete. Keep table metadata separate from computed results.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m3_2 -- --nocapture
cargo xtask accept --id cdoc-m3.2 --platform windows --require-all
cargo xtask accept --id cdoc-m3.2 --platform linux --require-all
cargo xtask accept --id cdoc-m3.2 --platform wasm --require-all
```
- `cdoc_m3_2__table_roundtrip`: Create, resize and rename a table then reopen in Excel without repair.
- `cdoc_m3_2__table_identity`: Column reorder preserves stable identity and dependent formulas are updated only through host operations.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m3.3 — Preserve shared array and dynamic-array formula encodings

**Type:** task · **Priority:** 1 · **Labels:** m3, cdoc, all

**Blocking prerequisites:** `cdoc-m1.3`, `cfml-m3.4`.
**Parent epic:** `cdoc-m3`.

### Outcome
Preserve shared array and dynamic-array formula encodings. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Read shared formula groups, legacy array ranges, dynamic array metadata and cached children without confusing authored and derived cells. Write changed groups by splitting/reindexing or expanding lawful formulas; never serialize a spill child as an independent authored formula.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m3_3 -- --nocapture
cargo xtask accept --id cdoc-m3.3 --platform windows --require-all
cargo xtask accept --id cdoc-m3.3 --platform linux --require-all
cargo xtask accept --id cdoc-m3.3 --platform wasm --require-all
```
- `cdoc_m3_3__shared_group_edit`: Editing one member preserves correct formulas in all other group members after Excel reopen.
- `cdoc_m3_3__spill_save`: The anchor formula, spill metadata and cached output shape reopen consistently without duplicate formulas.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m3.4 — Preserve external links richData and cell metadata

**Type:** task · **Priority:** 2 · **Labels:** m3, cdoc, all

**Blocking prerequisites:** `cdoc-m1.3`.
**Parent epic:** `cdoc-m3`.

### Outcome
Preserve external links richData and cell metadata. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Model external-link relationships and preserve richData, cm/vm metadata and unknown extension relationships with a fidelity ledger. Update references only for supported edits; refuse edits that cannot preserve required cross-part consistency.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m3_4 -- --nocapture
cargo xtask accept --id cdoc-m3.4 --platform windows --require-all
cargo xtask accept --id cdoc-m3.4 --platform linux --require-all
cargo xtask accept --id cdoc-m3.4 --platform wasm --require-all
```
- `cdoc_m3_4__metadata_roundtrip`: Untouched richData and external-link payloads retain their hashes and relationship targets.
- `cdoc_m3_4__unsafe_rewrite`: An edit requiring an unsupported metadata rewrite refuses explicitly rather than dropping metadata.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m3.5 — Model chart drawing pivot and page-setup attachment data

**Type:** task · **Priority:** 1 · **Labels:** m3, cdoc, all

**Blocking prerequisites:** `cdoc-m3.2`.
**Parent epic:** `cdoc-m3`.

### Outcome
Model chart drawing pivot and page-setup attachment data. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Define engine-neutral object IDs, anchor geometry, chart-series formula locations, pivot-source/cache descriptors and print/page metadata. Keep unknown parts losslessly; provide authoritative edit/relationship slots for later area modules without implementing rendering or refresh.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m3_5 -- --nocapture
cargo xtask accept --id cdoc-m3.5 --platform windows --require-all
cargo xtask accept --id cdoc-m3.5 --platform linux --require-all
cargo xtask accept --id cdoc-m3.5 --platform wasm --require-all
```
- `cdoc_m3_5__object_registry`: An opened workbook exposes real chart/pivot/page objects with resolvable IDs and referenced source ranges.
- `cdoc_m3_5__untouched_objects`: A scalar edit elsewhere preserves their untouched payloads and relationship graph.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m3.6 — Model styles themes rows columns and panes

**Type:** task · **Priority:** 1 · **Labels:** m3, cdoc, all

**Blocking prerequisites:** `cdoc-m1.3`.
**Parent epic:** `cdoc-m3`.

### Outcome
Model styles themes rows columns and panes. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Add interned styles/number formats/theme references, row/column dimensions and visibility, merges, outlines and view/pane state as sparse document metadata. Preserve style IDs on no-op save and avoid per-cell copies for shared styles.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m3_6 -- --nocapture
cargo xtask accept --id cdoc-m3.6 --platform windows --require-all
cargo xtask accept --id cdoc-m3.6 --platform linux --require-all
cargo xtask accept --id cdoc-m3.6 --platform wasm --require-all
```
- `cdoc_m3_6__style_identity`: An untouched style/shared-string table retains stable IDs and byte-preserved parts.
- `cdoc_m3_6__sparse_dimensions`: Setting one whole-column width does not create individual cell records.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m3 — Complete CalcHost — M3

**Type:** epic · **Priority:** 0 · **Labels:** m3, chost, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcHost in Workbook-wide references, arrays, names and extension attachment points have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 3 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## chost-m3.1 — Publish calculation generations and object revisions

**Type:** task · **Priority:** 1 · **Labels:** m3, chost, all

**Blocking prerequisites:** `chost-m2.4`, `ceng-m3.8`, `ceng-m3.4`.
**Parent epic:** `chost-m3`.

### Outcome
Publish calculation generations and object revisions. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Publish value/spill/dependency/object deltas with authored and calculation revisions. Add subscriptions for block anchors, ranges, chart-series and pivot-source objects. Reject stale worker responses and keep all skins on a coherent publication generation.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m3_1 -- --nocapture
cargo xtask accept --id chost-m3.1 --platform windows --require-all
cargo xtask accept --id chost-m3.1 --platform linux --require-all
cargo xtask accept --id chost-m3.1 --platform wasm --require-all
```
- `chost_m3_1__stale_tick`: An old recalc completion cannot overwrite a newer edit’s projection.
- `chost_m3_1__object_deltas`: An object source change sends the affected object revision and value deltas without rereading whole sheets.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m3.2 — Expose generic refresh and render attachment interfaces

**Type:** task · **Priority:** 2 · **Labels:** m3, chost, all

**Blocking prerequisites:** `chost-m3.1`.
**Parent epic:** `chost-m3`.

### Outcome
Expose generic refresh and render attachment interfaces. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Define host-owned refresh requests, transactional pivot output patches, immutable chart datasets and print snapshots. The production inspector reports pending/unsupported derived object state honestly; rendering/refresh adapters will be installed by separable-area implementation beads.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m3_2 -- --nocapture
cargo xtask accept --id chost-m3.2 --platform windows --require-all
cargo xtask accept --id chost-m3.2 --platform linux --require-all
cargo xtask accept --id chost-m3.2 --platform wasm --require-all
```
- `chost_m3_2__refresh_not_faked`: A refresh request without an implementation is a visible unsupported result, not a successful no-op.
- `chost_m3_2__snapshot_revision`: An object snapshot labels the precise authored/calculated/source revisions used.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m3.3 — Implement canonical named-function block editing

**Type:** task · **Priority:** 0 · **Labels:** m3, chost, all

**Blocking prerequisites:** `chost-m2.3`, `cdoc-m3.1`, `ceng-m3.2`, `cfml-m2.3`.
**Parent epic:** `chost-m3`.

### Outcome
Implement canonical named-function block editing. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Add the host commands to create a named-LAMBDA block, edit its single defined-name formula, attach labels/examples and invoke an explicit inspection request. Store no second executable definition in a worksheet text cell or layout manifest. Use the existing versioned operation/undo route and publish name/dependent deltas.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m3_3 -- --nocapture
cargo xtask accept --id chost-m3.3 --platform windows --require-all
cargo xtask accept --id chost-m3.3 --platform linux --require-all
cargo xtask accept --id chost-m3.3 --platform wasm --require-all
```
- `chost_m3_3__one_definition`: Editing a function block changes exactly its authoritative defined-name formula; dependent worksheet invocations recalculate through CalcFml.
- `chost_m3_3__excel_name_edit`: After real-Excel-compatible name edits are loaded, the block displays and invokes the new definition without resurrecting an old manifest body.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m3 — Complete DNA Calc — M3

**Type:** epic · **Priority:** 0 · **Labels:** m3, dnacalc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for DNA Calc in Workbook-wide references, arrays, names and extension attachment points have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## dnacalc-m3.1 — Complete named-callable and spill inspection across all skins

**Type:** task · **Priority:** 0 · **Labels:** m3, dnacalc, all

**Blocking prerequisites:** `dnacalc-m2.6`, `ceng-m3.2`, `ceng-m3.4`, `cfml-m3.4`, `chost-m3.1`, `chost-m3.3`.
**Parent epic:** `dnacalc-m3`.

### Outcome
Complete named-callable and spill inspection across all skins. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Workbook-wide references, arrays, names and extension attachment points. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Extend the existing shared detail/projection route with named-LAMBDA function blocks, explicit invocation inspection, raw-LAMBDA grid errors, spill anchors/children/blockers and range/name dependency navigation. A function block stores its canonical definition in an Excel defined name, not a duplicated executable body in both cell and manifest. The visible grid includes labels or invocation examples; editing the definition uses the same name command.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-skin-ir/ and DnaTreeCalc/src/dnacalc-shell/src/stage.rs: StageSurface. The source-manifest bead resolves exact intent/projection symbols. Replace tree-first WorkspaceState and the monolithic intent enum.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m3_1 -- --nocapture
cargo xtask accept --id dnacalc-m3.1 --platform windows --require-all
cargo xtask accept --id dnacalc-m3.1 --platform linux --require-all
cargo xtask accept --id dnacalc-m3.1 --platform wasm --require-all
```
- `dnacalc_m3_1__lambda_truth`: The same raw, defined-name and invoked LAMBDA cases have Excel-compatible grid outcomes and explicit callable inspection in all four skins.
- `dnacalc_m3_1__spill_truth`: Editing a real blocker changes the actual engine spill result and every interested skin through one delta; no skin fabricates the child values.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m4 — Complete Infrastructure — M4

**Type:** epic · **Priority:** 2 · **Labels:** m4, infra, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for Infrastructure in Structural editing, transactions and calculation settings have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## infra-m4.1 — Prove edit recalc and save failure consistency

**Type:** task · **Priority:** 2 · **Labels:** m4, infra, oracle

**Blocking prerequisites:** `chost-m4.9`, `chost-m4.8`, `ceng-m4.5`, `infra-m1.2`.
**Parent epic:** `infra-m4`.

### Outcome
Prove edit recalc and save failure consistency. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Retain randomized operation/reverse-operation traces covering row/column/sheet edits, names, tables, mode changes, failed recalc and refused save. Compare incremental state with replay from the same authored operation log using the same reference engine, then compare selected states to Excel.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m4.1 --platform windows --require-all
cargo xtask accept --id infra-m4.1 --platform linux --require-all
cargo xtask accept --id infra-m4.1 --platform wasm --require-all
cargo xtask accept --id infra-m4.1 --platform excel --require-all
```
- `infra_m4_1__transaction_matrix`: Injected recoverable failures at every preparation boundary preserve the committed revision pair.
- `infra_m4_1__replay_equivalence`: Replayed and incrementally maintained workbooks have equal authored content, calculated values and dependency facts.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m4 — Complete CalcFunc — M4

**Type:** epic · **Priority:** 2 · **Labels:** m4, cfunc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcFunc in Structural editing, transactions and calculation settings have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cfunc-m4.1 — Expose a source-neutral mutable UDF registry

**Type:** task · **Priority:** 2 · **Labels:** m4, cfunc, all

**Blocking prerequisites:** `cfunc-m3.22`.
**Parent epic:** `cfunc-m4`.

### Outcome
Expose a source-neutral mutable UDF registry. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Implement caller-owned RegistryState with register/unregister/invalidate generations, source IDs, caller context and volatility/thread-safety/async declarations. Built-in callable/OneCalc user-function registration uses it now; future sources are opaque IDs, not dead VBA/XLL enum variants.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/docs/function-lane/OXFUNC_UDF_REGISTRATION_AND_REGISTRY_MUTATION_CONTRACT.md: registry mutation contract (owner-brief reference); OxFunc/crates/oxfunc_core/src/function_call.rs: execution providers. Port contracts only after confirming code call sites.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m4_1 -- --nocapture
cargo xtask accept --id cfunc-m4.1 --platform windows --require-all
cargo xtask accept --id cfunc-m4.1 --platform linux --require-all
cargo xtask accept --id cfunc-m4.1 --platform wasm --require-all
```
- `cfunc_m4_1__registry_mutation`: Registering/replacing/unregistering a callable changes resolution and invalidates retained call-site handles.
- `cfunc_m4_1__source_neutral`: Two providers can register distinct source IDs without any platform-specific dependency.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m4 — Complete CalcFml — M4

**Type:** epic · **Priority:** 1 · **Labels:** m4, cfml, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcFml in Structural editing, transactions and calculation settings have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 3 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cfml-m4.1 — Transform references for copy fill move and paste

**Type:** task · **Priority:** 1 · **Labels:** m4, cfml, all

**Blocking prerequisites:** `cfml-m3.3`.
**Parent epic:** `cfml-m4`.

### Outcome
Transform references for copy fill move and paste. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Implement pure structural transforms over reference operands and source spans: copy versus cut/move, mixed absolute references, cross-sheet paste, discontiguous areas and formula reconstruction. Return affected binding/template facts; do not rewrite formulas by regex.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/reference_engine.rs: StrictExcelGridReferenceProfile, ExcelGridReferenceSystemProvider; OxFml/crates/oxfml_core/src/binding/profile.rs: reference requests/normal-form identity. Extract Excel semantics, not profiles or BTreeMap<String,...> providers.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m4_1 -- --nocapture
cargo xtask accept --id cfml-m4.1 --platform windows --require-all
cargo xtask accept --id cfml-m4.1 --platform linux --require-all
cargo xtask accept --id cfml-m4.1 --platform wasm --require-all
```
- `cfml_m4_1__copy_vs_move`: Captured Excel cases distinguish relative adjustment on copy from target-following on cut/move.
- `cfml_m4_1__mixed_anchors`: Filling formulas containing absolute rows/columns, names and quoted sheets preserves intended targets.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m4.2 — Transform references for cell axis and sheet lifecycle edits

**Type:** task · **Priority:** 1 · **Labels:** m4, cfml, all

**Blocking prerequisites:** `cfml-m4.1`.
**Parent epic:** `cfml-m4`.

### Outcome
Transform references for cell axis and sheet lifecycle edits. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Handle insert/delete cells, rows and columns; sheet add/rename/move/delete; range shrink/expansion, #REF tombstones and 3D endpoint rules. Apply the same transformer to cell formulas, names, CF/DV formulas and chart-series formula slots.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/reference_engine.rs: StrictExcelGridReferenceProfile, ExcelGridReferenceSystemProvider; OxFml/crates/oxfml_core/src/binding/profile.rs: reference requests/normal-form identity. Extract Excel semantics, not profiles or BTreeMap<String,...> providers.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m4_2 -- --nocapture
cargo xtask accept --id cfml-m4.2 --platform windows --require-all
cargo xtask accept --id cfml-m4.2 --platform linux --require-all
cargo xtask accept --id cfml-m4.2 --platform wasm --require-all
```
- `cfml_m4_2__delete_reference`: Deleting part/all of a referenced area produces the witnessed shrink or #REF form.
- `cfml_m4_2__noncell_formulas`: One sheet rename updates name, rule and series references using the same semantic transform.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m4.3 — Bind UDF generations and asynchronous result tokens

**Type:** task · **Priority:** 2 · **Labels:** m4, cfml, all

**Blocking prerequisites:** `cfml-m3.3`, `cfunc-m4.1`.
**Parent epic:** `cfml-m4`.

### Outcome
Bind UDF generations and asynchronous result tokens. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Compile registered function calls to generation-checked handles. Represent pending async results/cancellation tokens explicitly and report function-registry and caller dependencies. A production OneCalc callable registry and replay provider exercise the route without a VBA/XLL runtime.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/eval/mod.rs: compiled expressions and evaluation; OxFml/crates/oxfml_core/src/binding/profile.rs: ReferenceNormalFormKey, ReferenceFingerprintPolicy. Discard ReferencePolicy, host-reference syntax and the tree profile.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m4_3 -- --nocapture
cargo xtask accept --id cfml-m4.3 --platform windows --require-all
cargo xtask accept --id cfml-m4.3 --platform linux --require-all
cargo xtask accept --id cfml-m4.3 --platform wasm --require-all
```
- `cfml_m4_3__stale_handle`: A plan compiled before unregister/re-register cannot call a stale function implementation.
- `cfml_m4_3__pending_result`: A pending async replay result is distinguishable from a worksheet error or a calculated scalar.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m4 — Complete CalcEngine — M4

**Type:** epic · **Priority:** 1 · **Labels:** m4, ceng, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcEngine in Structural editing, transactions and calculation settings have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 6 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## ceng-m4.1 — Apply structural patches without rebuild-from-input

**Type:** task · **Priority:** 1 · **Labels:** m4, ceng, all

**Blocking prerequisites:** `ceng-m3.5`, `cdoc-m4.2`, `cfml-m4.2`.
**Parent epic:** `ceng-m4`.

### Outcome
Apply structural patches without rebuild-from-input. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Update coordinate-to-node maps, dependency descriptors, instantiated references and spill ownership from prepared patches. Keep stable node identities where appropriate; delete nodes explicitly. Use reference-impact indices rather than scanning unrelated formulas.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridAxisValueDependency, GridAxisVisibilityDependency, GridSheetSpanDependency and reverse range indexing. Reimplement with integer IDs in one workbook-wide graph; do not inherit the old per-sheet scheduler.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m4_1 -- --nocapture
cargo xtask accept --id ceng-m4.1 --platform windows --require-all
cargo xtask accept --id ceng-m4.1 --platform linux --require-all
cargo xtask accept --id ceng-m4.1 --platform wasm --require-all
```
- `ceng_m4_1__delete_not_rebuild`: Deleting a cell/row retains independent graph nodes and compiled plan identities while updating affected dependencies.
- `ceng_m4_1__undo_structural`: Applying an inverse structural patch restores graph semantics and recomputed values.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m4.2 — Implement calculation modes and recalc command scopes

**Type:** task · **Priority:** 1 · **Labels:** m4, ceng, all

**Blocking prerequisites:** `ceng-m4.1`, `ceng-m3.6`.
**Parent epic:** `ceng-m4`.

### Outcome
Implement calculation modes and recalc command scopes. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Implement automatic, automatic-except-data-tables and manual policies; ordinary F9, active-sheet Shift+F9, forced full Ctrl+Alt+F9 and dependency-rebuild Ctrl+Alt+Shift+F9. Model application-level recalc requests in CalcHost across open workbooks. Preserve cached stale values in manual mode with truthful provenance.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m4_2 -- --nocapture
cargo xtask accept --id ceng-m4.2 --platform windows --require-all
cargo xtask accept --id ceng-m4.2 --platform linux --require-all
cargo xtask accept --id ceng-m4.2 --platform wasm --require-all
```
- `ceng_m4_2__manual_scope`: An edit in manual mode marks dependencies dirty without silently calculating; F9 then computes the requested scope.
- `ceng_m4_2__full_vs_rebuild`: Force-full reevaluates all formulas without rebuilding input state; dependency rebuild is a distinct counted command.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m4.3 — Apply date precision and calcPr policies

**Type:** task · **Priority:** 1 · **Labels:** m4, ceng, all

**Blocking prerequisites:** `ceng-m4.2`, `cfml-m2.6`, `cfunc-m3.7`.
**Parent epic:** `ceng-m4`.

### Outcome
Apply date precision and calcPr policies. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Carry workbook 1900/1904 systems, iteration settings, compatibility version, calcPr flags and full-recalc-on-open policy in EvalEnvironment. Implement precision-as-displayed with captured persistence/mutation semantics, not merely a display-rounding shortcut; formatting edits can affect calculation under this flag.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m4_3 -- --nocapture
cargo xtask accept --id ceng-m4.3 --platform windows --require-all
cargo xtask accept --id ceng-m4.3 --platform linux --require-all
cargo xtask accept --id ceng-m4.3 --platform wasm --require-all
```
- `ceng_m4_3__precision_display`: Captured values and saved results demonstrate the selected formatting policy affects calculation as Excel does.
- `ceng_m4_3__date_switch`: Date-system changes invalidate the appropriate calculations and update serialized settings.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m4.4 — Invalidate all numeric results on mode change

**Type:** task · **Priority:** 2 · **Labels:** m4, ceng, all

**Blocking prerequisites:** `ceng-m4.3`, `cfunc-m3.23`.
**Parent epic:** `ceng-m4`.

### Outcome
Invalidate all numeric results on mode change. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Carry NumericMode/target in all evaluation and cache keys. A mode switch invalidates formula results, dependent names, array shapes, rules and derived-object calculations, and mode-sensitive bound handles. Reuse mode-invariant syntax plans. A failed capability check changes neither workbook policy nor values.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m4_4 -- --nocapture
cargo xtask accept --id ceng-m4.4 --platform windows --require-all
cargo xtask accept --id ceng-m4.4 --platform linux --require-all
cargo xtask accept --id ceng-m4.4 --platform wasm --require-all
```
- `ceng_m4_4__mode_full_recalc`: Switching mode reevaluates every formula and labels the resulting generation correctly.
- `ceng_m4_4__unavailable_atomic`: Requesting Exact on wasm returns NumericModeUnavailable and preserves both authored revision and prior selected mode.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m4.5 — Publish counted escalation facts and replay traces

**Type:** task · **Priority:** 2 · **Labels:** m4, ceng, all

**Blocking prerequisites:** `ceng-m4.1`.
**Parent epic:** `ceng-m4`.

### Outcome
Publish counted escalation facts and replay traces. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Return typed RecalcFacts with evaluated IDs, invalidation causes, reschedules, SCC changes, full-recalc reasons and escalation counts. Every conservative broadening has a stable reason and replacement-owner identifier; no escalation is a silently successful ordinary edit.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m4_5 -- --nocapture
cargo xtask accept --id ceng-m4.5 --platform windows --require-all
cargo xtask accept --id ceng-m4.5 --platform linux --require-all
cargo xtask accept --id ceng-m4.5 --platform wasm --require-all
```
- `ceng_m4_5__escalation_count`: An injected conservative branch increments a named counter and appears in the receipt.
- `ceng_m4_5__evaluation_witness`: The evaluated-node set agrees with an independent CalcFml call observer and cannot include file-cached-only cells.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m4.6 — Apply external RTD and async completion operations

**Type:** task · **Priority:** 2 · **Labels:** m4, ceng, all

**Blocking prerequisites:** `ceng-m3.9`, `cfml-m4.3`.
**Parent epic:** `ceng-m4`.

### Outcome
Apply external RTD and async completion operations. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Accept revisioned ExternalValueChanged, RtdChanged and AsyncCompleted operations with source/subscription IDs. Reject stale completions and invalidate subscribed nodes separately from volatile recalc. Use the production capture/replay adapter as the phase-1 provider.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m4_6 -- --nocapture
cargo xtask accept --id ceng-m4.6 --platform windows --require-all
cargo xtask accept --id ceng-m4.6 --platform linux --require-all
cargo xtask accept --id ceng-m4.6 --platform wasm --require-all
```
- `ceng_m4_6__external_vs_volatile`: An RTD update reevaluates its subscribers without simulating a volatile full tick.
- `ceng_m4_6__stale_completion`: A cancelled or superseded async result cannot overwrite the current result generation.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m4 — Complete CalcDoc — M4

**Type:** epic · **Priority:** 1 · **Labels:** m4, cdoc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcDoc in Structural editing, transactions and calculation settings have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 3 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cdoc-m4.1 — Implement persistent sparse pages and axis order maps

**Type:** task · **Priority:** 1 · **Labels:** m4, cdoc, all

**Blocking prerequisites:** `cdoc-m3.3`.
**Parent epic:** `cdoc-m4`.

### Outcome
Implement persistent sparse pages and axis order maps. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Use bounded sparse cell pages, stable row/column slot identities and order-statistic axis maps. Apply value edits by copying only touched pages; structural inserts update axis mappings plus affected descriptors. Retain immutable snapshots for save/undo with explicit ownership and release.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m4_1 -- --nocapture
cargo xtask accept --id cdoc-m4.1 --platform windows --require-all
cargo xtask accept --id cdoc-m4.1 --platform linux --require-all
cargo xtask accept --id cdoc-m4.1 --platform wasm --require-all
```
- `cdoc_m4_1__untouched_page_identity`: A one-cell edit preserves identities of all untouched document pages.
- `cdoc_m4_1__axis_insert`: Inserting a blank row before a far sparse cell does not scan or duplicate every blank row.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m4.2 — Prepare reversible cell range and axis patches

**Type:** task · **Priority:** 1 · **Labels:** m4, cdoc, all

**Blocking prerequisites:** `cdoc-m4.1`, `cfml-m4.2`.
**Parent epic:** `cdoc-m4`.

### Outcome
Prepare reversible cell range and axis patches. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Prepare add/clear/fill/move/range and row/column insert/delete patches with inverse authored data. Keep formula transformation orchestration in CalcHost/CalcFml; CalcDoc stores resolved semantic patches and source-dirty regions. Preflight resource reservations before commit.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m4_2 -- --nocapture
cargo xtask accept --id cdoc-m4.2 --platform windows --require-all
cargo xtask accept --id cdoc-m4.2 --platform linux --require-all
cargo xtask accept --id cdoc-m4.2 --platform wasm --require-all
```
- `cdoc_m4_2__patch_inverse`: Apply then inverse produces byte-equivalent authored model content and stable unaffected IDs.
- `cdoc_m4_2__prepare_oom`: Injected reservation failure leaves the original model and source-dirty ledger unchanged.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m4.3 — Author sheet lifecycle and relationship changes

**Type:** task · **Priority:** 2 · **Labels:** m4, cdoc, all

**Blocking prerequisites:** `cdoc-m4.2`.
**Parent epic:** `cdoc-m4`.

### Outcome
Author sheet lifecycle and relationship changes. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Create, duplicate, rename, reorder, hide and delete worksheets with correct relationships, content types, names, table/object references and active-sheet state. Preserve unrelated parts and author fresh workbook packages, not only edits to existing XML.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m4_3 -- --nocapture
cargo xtask accept --id cdoc-m4.3 --platform windows --require-all
cargo xtask accept --id cdoc-m4.3 --platform linux --require-all
cargo xtask accept --id cdoc-m4.3 --platform wasm --require-all
```
- `cdoc_m4_3__fresh_export`: Create a two-sheet workbook from no source package and reopen it in Excel without repair.
- `cdoc_m4_3__sheet_delete`: Deleting a sheet removes only owned parts and leaves unrelated drawing/customXml relationships valid.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4 — Complete CalcHost — M4

**Type:** epic · **Priority:** 1 · **Labels:** m4, chost, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcHost in Structural editing, transactions and calculation settings have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 13 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## chost-m4.1 — Prepare one atomic operation stream for structural edits

**Type:** task · **Priority:** 1 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m3.1`, `ceng-m4.1`.
**Parent epic:** `chost-m4`.

### Outcome
Prepare one atomic operation stream for structural edits. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Prepare one canonical authored patch using CalcDoc and CalcFml transformations; derive the engine update from that same patch and operation identity. Commit document/engine revision changes together after preflight. Include names/rules/tables/object formula slots in reference-impact lookup.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_1 -- --nocapture
cargo xtask accept --id chost-m4.1 --platform windows --require-all
cargo xtask accept --id chost-m4.1 --platform linux --require-all
cargo xtask accept --id chost-m4.1 --platform wasm --require-all
```
- `chost_m4_1__one_stream`: UI and headless execution of the same structural op produce identical canonical patches and graph changes.
- `chost_m4_1__failure_at_each_step`: Fault injection at every fallible preparation boundary leaves both states unchanged.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.2 — Implement core range editing and clipboard commands

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.1`, `cfml-m4.1`.
**Parent epic:** `chost-m4`.

### Outcome
Implement core range editing and clipboard commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Implement clear contents/formats/all, range copy/cut/move/paste, Paste Special value/formula/format/operation/transposition modes, insert/delete cells and fill-series commands. Use one undo group per user gesture and reject partial writes into owned spill/array regions.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_2 -- --nocapture
cargo xtask accept --id chost-m4.2 --platform windows --require-all
cargo xtask accept --id chost-m4.2 --platform linux --require-all
cargo xtask accept --id chost-m4.2 --platform wasm --require-all
```
- `chost_m4_2__paste_special`: Formula/value/format and transpose paste produce independently checked model/results.
- `chost_m4_2__spill_edit_rejected`: An illegal write to a spill child leaves the complete transaction unapplied.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.3 — Implement sheet name table and workbook lifecycle commands

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.1`, `cdoc-m4.3`, `cdoc-m3.2`.
**Parent epic:** `chost-m4`.

### Outcome
Implement sheet name table and workbook lifecycle commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Expose sheet create/copy/rename/reorder/hide/delete, defined-name create/rename/delete and structured-table lifecycle through the command registry. Maintain identity-based selections/subscriptions and repair view locators via the same patch.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_3 -- --nocapture
cargo xtask accept --id chost-m4.3 --platform windows --require-all
cargo xtask accept --id chost-m4.3 --platform linux --require-all
cargo xtask accept --id chost-m4.3 --platform wasm --require-all
```
- `chost_m4_3__lifecycle_undo`: Rename/reorder/delete and undo restore scoped names, tables, formulas and view anchors consistently.
- `chost_m4_3__duplicate_name`: Invalid or conflicting sheet/table/name requests are rejected atomically with useful diagnostics.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.4 — Implement sort filter and visibility commands

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.2`, `ceng-m3.5`.
**Parent epic:** `chost-m4`.

### Outcome
Implement sort filter and visibility commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Implement multi-key sort, custom/list sorts, AutoFilter and advanced-filter core commands with stable data-row moves and formula/reference semantics. Separate manually hidden, filtered and outline-hidden state; update visibility dependencies and command undo receipts.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_4 -- --nocapture
cargo xtask accept --id chost-m4.4 --platform windows --require-all
cargo xtask accept --id chost-m4.4 --platform linux --require-all
cargo xtask accept --id chost-m4.4 --platform wasm --require-all
```
- `chost_m4_4__sort_formula_rows`: Sorting a mixed-value/formula table preserves observed Excel reference behaviour.
- `chost_m4_4__visibility_recalc`: Filtering or hiding rows updates SUBTOTAL/AGGREGATE through visibility dependencies without a full rebuild.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.5 — Implement core cell formatting and style commands

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.1`, `cfml-m2.6`, `cdoc-m3.6`.
**Parent epic:** `chost-m4`.

### Outcome
Implement core cell formatting and style commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Add number/alignment/font/border/fill/protection style patches, merge/unmerge, cell styles and theme references; formatting is part of authored document state. Route numeric text/precision effects through CalcFml and the engine rather than evaluating in UI.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_5 -- --nocapture
cargo xtask accept --id chost-m4.5 --platform windows --require-all
cargo xtask accept --id chost-m4.5 --platform linux --require-all
cargo xtask accept --id chost-m4.5 --platform wasm --require-all
```
- `chost_m4_5__style_undo`: Applying and undoing a range style restores effective formats without copying all cell values.
- `chost_m4_5__precision_notification`: A number-format change under precision-as-displayed triggers the required calculation effect.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.6 — Implement find replace and navigation query commands

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.2`.
**Parent epic:** `chost-m4`.

### Outcome
Implement find replace and navigation query commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Implement Find/Replace over formulas versus values, wildcard/case/whole-cell options, Go To and Go To Special selection predicates. Query sparse indexes and active scopes; replacements are one validated transaction with formula transforms as needed.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_6 -- --nocapture
cargo xtask accept --id chost-m4.6 --platform windows --require-all
cargo xtask accept --id chost-m4.6 --platform linux --require-all
cargo xtask accept --id chost-m4.6 --platform wasm --require-all
```
- `chost_m4_6__replace_scope`: Replacing in formulas on one sheet does not alter text values or other sheets unexpectedly.
- `chost_m4_6__go_to_special`: Constants, formulas, blanks, errors and visible-cell queries return the correct sparse selections.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.7 — Implement workbook protection and permission checks

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.3`, `chost-m4.5`.
**Parent epic:** `chost-m4`.

### Outcome
Implement workbook protection and permission checks. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Apply sheet/workbook protection settings at the command boundary for every skin/headless caller; model locked/unlocked cells and allowed actions. Distinguish editing protection from confidentiality/encryption. Save corresponding workbook/style metadata faithfully.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_7 -- --nocapture
cargo xtask accept --id chost-m4.7 --platform windows --require-all
cargo xtask accept --id chost-m4.7 --platform linux --require-all
cargo xtask accept --id chost-m4.7 --platform wasm --require-all
```
- `chost_m4_7__headless_protection`: A prohibited write fails identically from grid, notebook and object-dispatch commands.
- `chost_m4_7__allowed_cells`: An unlocked allowed cell remains editable and undo does not bypass protection policy.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.8 — Expose application-level recalc and numeric preferences

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.1`, `ceng-m4.4`.
**Parent epic:** `chost-m4`.

### Outcome
Expose application-level recalc and numeric preferences. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Persist requested NumericMode/target as DNA metadata per workbook; record effective session capability separately. Default every app and platform to Fast; Exact is an explicit per-workbook selection presented with its availability and evidence status (plan section 5.1). Never silently downgrade a saved Exact workbook: prompt/refuse calculation until explicit user mode selection. Coordinate F9 across open workbooks.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_8 -- --nocapture
cargo xtask accept --id chost-m4.8 --platform windows --require-all
cargo xtask accept --id chost-m4.8 --platform linux --require-all
cargo xtask accept --id chost-m4.8 --platform wasm --require-all
```
- `chost_m4_8__saved_mode`: Opening a saved Exact workbook on wasm leaves it explicitly uncalculated/unavailable until a user selects Fast.
- `chost_m4_8__application_f9`: The host application command reaches each eligible open workbook under its settings and collects truthful receipts.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.9 — Complete bounded undo redo and save snapshot retention

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.1`.
**Parent epic:** `chost-m4`.

### Outcome
Complete bounded undo redo and save snapshot retention. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Use inverse patches plus bounded snapshot references, redo invalidation and transaction coalescing. Release obsolete pages/engine snapshots when history entries expire. A refused save or cancelled recalc does not append a fake undo entry; a save concurrent with later edits cannot clear their dirty status.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_9 -- --nocapture
cargo xtask accept --id chost-m4.9 --platform windows --require-all
cargo xtask accept --id chost-m4.9 --platform linux --require-all
cargo xtask accept --id chost-m4.9 --platform wasm --require-all
```
- `chost_m4_9__history_release`: Evicted undo entries release their unique page references under independent ownership instrumentation.
- `chost_m4_9__save_failure`: Refused or failed save leaves current authored/engine state and undo/redo order unchanged.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.10 — Coordinate multiple open documents and cross-book transactions

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.8`, `ceng-m3.9`.
**Parent epic:** `chost-m4`.

### Outcome
Coordinate multiple open documents and cross-book transactions. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Maintain a Host-owned DocumentSet plus one calculation domain for connected open books. Route commands with workbook IDs, prepare all affected document revisions before atomic cross-book commit, and publish per-workbook deltas with the shared revision vector. Save remains per-document unless an explicit multi-save is requested.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_10 -- --nocapture
cargo xtask accept --id chost-m4.10 --platform windows --require-all
cargo xtask accept --id chost-m4.10 --platform linux --require-all
cargo xtask accept --id chost-m4.10 --platform wasm --require-all
```
- `chost_m4_10__cross_book_atomic`: A multi-workbook edit either commits all requested authored patches and graph effects or none.
- `chost_m4_10__one_book_save`: Saving one book does not mark another dirty book saved or discard shared dependency state.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.11 — Add events and exclusive batch execution mode

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.10`, `ceng-m4.6`.
**Parent epic:** `chost-m4`.

### Outcome
Add events and exclusive batch execution mode. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Publish ordered open/change/calculate/save events after the relevant commits. Provide an exclusive serialized BatchExecute transaction mode with queued external updates and bounded event reentrancy. The production headless batch command uses it now; future macros use the same facility.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxVba/crates/oxvba-host/src/engine.rs: host callback integration (owner-brief reference). Implement a platform-neutral attachment only; no VBA, COM, MSForms or XLL runtime is built in this phase.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_11 -- --nocapture
cargo xtask accept --id chost-m4.11 --platform windows --require-all
cargo xtask accept --id chost-m4.11 --platform linux --require-all
cargo xtask accept --id chost-m4.11 --platform wasm --require-all
```
- `chost_m4_11__event_order`: Change precedes Calculate for an edit/recalc and observers see committed state only.
- `chost_m4_11__exclusive_batch`: Concurrent external updates are queued and applied in order after the exclusive batch, with no lost update.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.12 — Expose object dispatch over host commands

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.11`.
**Parent epic:** `chost-m4`.

### Outcome
Expose object dispatch over host commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Define Application/Workbook/Worksheet/Range object handles with generations and late-bound member dispatch translated into canonical commands/queries. A production calcctl object command and formula range inspector call it. Keep COM types and IDispatch outside the portable host.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxVba/crates/oxvba-host/src/engine.rs: host callback integration (owner-brief reference). Implement a platform-neutral attachment only; no VBA, COM, MSForms or XLL runtime is built in this phase.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_12 -- --nocapture
cargo xtask accept --id chost-m4.12 --platform windows --require-all
cargo xtask accept --id chost-m4.12 --platform linux --require-all
cargo xtask accept --id chost-m4.12 --platform wasm --require-all
```
- `chost_m4_12__object_write`: Writing Range.Value through headless object dispatch produces the same operation/undo receipt as grid entry.
- `chost_m4_12__stale_object`: A handle to a deleted sheet/range fails with a typed stale-object error.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m4.13 — Reserve add-in callback capabilities IDE panels and range picking

**Type:** task · **Priority:** 2 · **Labels:** m4, chost, all

**Blocking prerequisites:** `chost-m4.12`.
**Parent epic:** `chost-m4`.

### Outcome
Reserve add-in callback capabilities IDE panels and range picking. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Structural editing, transactions and calculation settings. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Expose typed host-service queries/commands suitable for later Excel12v adaptation; add capability discovery, range-selection sessions and dock/panel registration. The production formula/range picker and diagnostics panel consume these now. Leave XLOPER12 marshalling, COM, VBA, forms and native loading outside phase 1.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxVba/crates/oxvba-host/src/engine.rs: host callback integration (owner-brief reference). Implement a platform-neutral attachment only; no VBA, COM, MSForms or XLL runtime is built in this phase.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m4_13 -- --nocapture
cargo xtask accept --id chost-m4.13 --platform windows --require-all
cargo xtask accept --id chost-m4.13 --platform linux --require-all
cargo xtask accept --id chost-m4.13 --platform wasm --require-all
```
- `chost_m4_13__range_picker`: A formula dialog requests a range, receives selection/cancel through the shared host service and creates no separate workbook model.
- `chost_m4_13__capability_discovery`: The shipped shell reports unavailable VBA/XLL capabilities honestly while using the real generic panel/service route.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m5 — Complete Infrastructure — M5

**Type:** epic · **Priority:** 3 · **Labels:** m5, infra, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for Infrastructure in Iterative calculation, data tables, conditional formatting and validation have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 3 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## infra-m5.1 — Capture iterative and data-table scheduling witnesses

**Type:** task · **Priority:** 3 · **Labels:** m5, infra, oracle

**Blocking prerequisites:** `infra-m1.2`.
**Parent epic:** `infra-m5`.

### Outcome
Capture iterative and data-table scheduling witnesses. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Capture small SCCs with different cell/name order, seeds, damping, divergence, error/text outputs and mode settings; capture 1D/2D TABLE cases, cyclic tables, volatile inputs and automatic-except-tables. Retain the actual Excel iteration/results observations and ambiguity notes, not hard-coded production response tables.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m5.1 --platform windows --require-all
cargo xtask accept --id infra-m5.1 --platform linux --require-all
cargo xtask accept --id infra-m5.1 --platform wasm --require-all
cargo xtask accept --id infra-m5.1 --platform excel --require-all
```
- `infra_m5_1__iteration_witnesses`: Each case records formulas, settings, initial state, observed outputs and target fingerprint.
- `infra_m5_1__table_witnesses`: One- and two-variable tables plus cyclic/volatile cases have independently captured expected values and no fabricated trace claims.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m5.2 — Capture CF DV and precision-as-displayed witnesses

**Type:** task · **Priority:** 3 · **Labels:** m5, infra, oracle

**Blocking prerequisites:** `infra-m1.2`.
**Parent epic:** `infra-m5`.

### Outcome
Capture CF DV and precision-as-displayed witnesses. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Create independent Excel captures for relative rule anchors, priorities, aggregate thresholds, DV edit/paste behaviours, date systems and precision-as-displayed changes/saves. Include cross-sheet defined names and full-column applicability.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m5.2 --platform windows --require-all
cargo xtask accept --id infra-m5.2 --platform linux --require-all
cargo xtask accept --id infra-m5.2 --platform wasm --require-all
cargo xtask accept --id infra-m5.2 --platform excel --require-all
```
- `infra_m5_2__rule_captures`: Expected rules, visible classifications and rejected/accepted edit outcomes come from real Excel observations.
- `infra_m5_2__precision_capture`: The pre/post save values and formatting flags are retained so display rounding cannot masquerade as calculation semantics.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m5.3 — Prove hard-calculation first-version coverage

**Type:** task · **Priority:** 3 · **Labels:** m5, infra, oracle

**Blocking prerequisites:** `chost-m5.1`, `chost-m5.2`, `chost-m5.3`, `infra-m5.2`, `ceng-m5.9`.
**Parent epic:** `infra-m5`.

### Outcome
Prove hard-calculation first-version coverage. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Run the product HostState path across the SCC, TABLE, CF and DV capture suites on native and wasm. Count all required participant instances independently. Require calculation/evaluation, not preservation or cached outcomes, for these mandatory features.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m5.3 --platform windows --require-all
cargo xtask accept --id infra-m5.3 --platform linux --require-all
cargo xtask accept --id infra-m5.3 --platform wasm --require-all
cargo xtask accept --id infra-m5.3 --platform excel --require-all
```
- `infra_m5_3__hard_features_calculate`: All required fixture participants execute the real evaluator and match their recorded outputs/errors/shapes.
- `infra_m5_3__no_typed_unsupported_exit`: A mandatory SCC/TABLE/CF/DV fixture marked unsupported prevents this milestone from passing.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m5 — Complete CalcFml — M5

**Type:** epic · **Priority:** 3 · **Labels:** m5, cfml, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcFml in Iterative calculation, data tables, conditional formatting and validation have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cfml-m5.1 — Compile per-cell rule contexts for CF and DV

**Type:** task · **Priority:** 3 · **Labels:** m5, cfml, all

**Blocking prerequisites:** `cfml-m4.2`, `cdoc-m5.2`, `cdoc-m5.3`.
**Parent epic:** `cfml-m5`.

### Outcome
Compile per-cell rule contexts for CF and DV. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Compile shared rule templates with applicability anchor and target-cell contexts. Reuse the same coercion, reference and name semantics as ordinary formulas; expose rule read footprints without generating a formula object for every blank cell in a whole-column applies-to range.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/eval/mod.rs: compiled expressions and evaluation; OxFml/crates/oxfml_core/src/binding/profile.rs: ReferenceNormalFormKey, ReferenceFingerprintPolicy. Discard ReferencePolicy, host-reference syntax and the tree profile.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m5_1 -- --nocapture
cargo xtask accept --id cfml-m5.1 --platform windows --require-all
cargo xtask accept --id cfml-m5.1 --platform linux --require-all
cargo xtask accept --id cfml-m5.1 --platform wasm --require-all
```
- `cfml_m5_1__rule_relative`: Applying the same rule to two cells instantiates different correct relative reads.
- `cfml_m5_1__rule_sharing`: A whole-column rule shares one template and does not materialize one million plans.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m5 — Complete CalcEngine — M5

**Type:** epic · **Priority:** 3 · **Labels:** m5, ceng, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcEngine in Iterative calculation, data tables, conditional formatting and validation have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 9 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## ceng-m5.1 — Detect and maintain workbook strongly connected components

**Type:** task · **Priority:** 3 · **Labels:** m5, ceng, all

**Blocking prerequisites:** `ceng-m4.1`.
**Parent epic:** `ceng-m5`.

### Outcome
Detect and maintain workbook strongly connected components. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Implement SCC classification over cell, contextual-name and dynamic-reference participants. Update affected graph components after edits and dynamic edge changes; use a deterministic component/order policy. Non-iterative cycles return the observed circular-reference/error policy.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridAxisValueDependency, GridAxisVisibilityDependency, GridSheetSpanDependency and reverse range indexing. Reimplement with integer IDs in one workbook-wide graph; do not inherit the old per-sheet scheduler.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m5_1 -- --nocapture
cargo xtask accept --id ceng-m5.1 --platform windows --require-all
cargo xtask accept --id ceng-m5.1 --platform linux --require-all
cargo xtask accept --id ceng-m5.1 --platform wasm --require-all
```
- `ceng_m5_1__name_cell_scc`: A cycle spanning two sheets and a defined name is one SCC.
- `ceng_m5_1__dynamic_scc_change`: Changing an INDIRECT target can create or break a cycle and classification changes before publication.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m5.2 — Implement captured iterative scheduling and convergence

**Type:** task · **Priority:** 3 · **Labels:** m5, ceng, all

**Blocking prerequisites:** `ceng-m5.1`, `infra-m5.1`.
**Parent epic:** `ceng-m5`.

### Outcome
Implement captured iterative scheduling and convergence. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Implement the chosen target’s observed iteration initialization, node ordering/update visibility, maximum-iteration and maximum-change policy. Use CalcFml for every iteration. Handle mixed numeric/error/text values and early cancellation explicitly; record iterations and termination reason.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m5_2 -- --nocapture
cargo xtask accept --id ceng-m5.2 --platform windows --require-all
cargo xtask accept --id ceng-m5.2 --platform linux --require-all
cargo xtask accept --id ceng-m5.2 --platform wasm --require-all
```
- `ceng_m5_2__iterative_excel`: All committed SCC witness cases reproduce captured outputs and termination classifications.
- `ceng_m5_2__not_literal_lookup`: Perturb an input outside the capture list and verify recurrence steps through an independent evaluation observer rather than a table lookup.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m5.3 — Handle iterative arrays dynamic topology and failure publication

**Type:** task · **Priority:** 3 · **Labels:** m5, ceng, all

**Blocking prerequisites:** `ceng-m5.2`, `ceng-m3.4`.
**Parent epic:** `ceng-m5`.

### Outcome
Handle iterative arrays dynamic topology and failure publication. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Specify and implement shape/dynamic-edge changes during iterative calculation using retained witnesses. Reclassify components when needed, count reschedules and keep publication atomic. Resource cancellation/divergence never substitutes a file-cached final answer.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m5_3 -- --nocapture
cargo xtask accept --id ceng-m5.3 --platform windows --require-all
cargo xtask accept --id ceng-m5.3 --platform linux --require-all
cargo xtask accept --id ceng-m5.3 --platform wasm --require-all
```
- `ceng_m5_3__iteration_shape`: An iterative formula that changes a spill/dynamic reference follows the independently captured behaviour for every admitted mandatory witness; an unsupported substitute fails this task.
- `ceng_m5_3__iteration_cancel`: Cancellation leaves the previous published generation stale and authored changes intact.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m5.4 — Evaluate one-variable data tables with input overlays

**Type:** task · **Priority:** 3 · **Labels:** m5, ceng, all

**Blocking prerequisites:** `ceng-m4.2`, `cdoc-m5.1`, `infra-m5.1`, `ceng-m3.6`.
**Parent epic:** `ceng-m5`.

### Outcome
Evaluate one-variable data tables with input overlays. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Implement one-variable data-table execution as temporary input overrides and nested work items on the same graph/evaluator. Restore ordinary workbook inputs after each run, union required read facts and publish the complete output block as one calculation generation.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m5_4 -- --nocapture
cargo xtask accept --id ceng-m5.4 --platform windows --require-all
cargo xtask accept --id ceng-m5.4 --platform linux --require-all
cargo xtask accept --id ceng-m5.4 --platform wasm --require-all
```
- `ceng_m5_4__one_variable_table`: Column- and row-oriented tables match independent Excel captures.
- `ceng_m5_4__override_isolation`: Temporary input values are never committed to authored cells or seen by a later unrelated calculation.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m5.5 — Evaluate two-variable data tables and cyclic table semantics

**Type:** task · **Priority:** 3 · **Labels:** m5, ceng, all

**Blocking prerequisites:** `ceng-m5.4`, `ceng-m5.2`.
**Parent epic:** `ceng-m5`.

### Outcome
Evaluate two-variable data tables and cyclic table semantics. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Extend the same execution path to two-variable combinations, ordering, table self-dependency and captured volatile/iterative interactions. Honor automatic-except-tables and explicit F9 triggers without recreating the document per scenario.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m5_5 -- --nocapture
cargo xtask accept --id ceng-m5.5 --platform windows --require-all
cargo xtask accept --id ceng-m5.5 --platform linux --require-all
cargo xtask accept --id ceng-m5.5 --platform wasm --require-all
```
- `ceng_m5_5__two_variable_table`: Every output of the captured 2D table is computed from actual scenario evaluation.
- `ceng_m5_5__table_cycle`: The witnessed table self-dependency is handled using table semantics rather than rejected as an ordinary graph cycle.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m5.6 — Register conditional-formatting dependency participants

**Type:** task · **Priority:** 3 · **Labels:** m5, ceng, all

**Blocking prerequisites:** `ceng-m3.1`, `cfml-m5.1`.
**Parent epic:** `ceng-m5`.

### Outcome
Register conditional-formatting dependency participants. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Represent rule-template applicability and evaluated target-cell instances in the graph. Track conditional numeric/text/date/formula predicates and view-demand evaluations, including rule priority/stop-if-true. Invalidation is compressed over applicable regions.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridAxisValueDependency, GridAxisVisibilityDependency, GridSheetSpanDependency and reverse range indexing. Reimplement with integer IDs in one workbook-wide graph; do not inherit the old per-sheet scheduler.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m5_6 -- --nocapture
cargo xtask accept --id ceng-m5.6 --platform windows --require-all
cargo xtask accept --id ceng-m5.6 --platform linux --require-all
cargo xtask accept --id ceng-m5.6 --platform wasm --require-all
```
- `ceng_m5_6__cf_recalc`: Changing a referenced cell updates the affected rule result delivered to the skin.
- `ceng_m5_6__no_dense_rule_grid`: A whole-column rule stores compressed applicability plus requested instances, not one node per blank cell.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m5.7 — Evaluate color scales data bars and icon sets

**Type:** task · **Priority:** 3 · **Labels:** m5, ceng, all

**Blocking prerequisites:** `ceng-m5.6`, `cfunc-m3.10`.
**Parent epic:** `ceng-m5`.

### Outcome
Evaluate color scales data bars and icon sets. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Add aggregate rule participants for range-dependent thresholds/percentiles, explicit threshold formulas and per-cell style classifications. Keep conditional formatting separate from authored base styles and prohibit circular display-precision feedback unless captured semantics define it.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m5_7 -- --nocapture
cargo xtask accept --id ceng-m5.7 --platform windows --require-all
cargo xtask accept --id ceng-m5.7 --platform linux --require-all
cargo xtask accept --id ceng-m5.7 --platform wasm --require-all
```
- `ceng_m5_7__scale_thresholds`: Changing a range extremum updates the dependent color/bar/icon facts according to captured Excel thresholds.
- `ceng_m5_7__priority_stack`: Multiple conflicting rules obey priority and stop-if-true without mutating base styles.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m5.8 — Evaluate validation candidates in staged input contexts

**Type:** task · **Priority:** 3 · **Labels:** m5, ceng, all

**Blocking prerequisites:** `ceng-m3.3`, `cfml-m5.1`, `cfunc-m3.21`.
**Parent epic:** `ceng-m5`.

### Outcome
Evaluate validation candidates in staged input contexts. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Evaluate DV against a proposed transaction snapshot and the appropriate old/new value context from oracle witnesses. Register dependencies for rule indicators; treat validation prompting as host policy, not automatic rejection of all programmatic writes. Support allow-blank and stop/warning/information outcomes.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m5_8 -- --nocapture
cargo xtask accept --id ceng-m5.8 --platform windows --require-all
cargo xtask accept --id ceng-m5.8 --platform linux --require-all
cargo xtask accept --id ceng-m5.8 --platform wasm --require-all
```
- `ceng_m5_8__validation_candidate`: A candidate referring to another edited cell in the same batch sees the correct staged value.
- `ceng_m5_8__programmatic_policy`: Typed validation results distinguish a UI stop rejection from an explicitly permitted programmatic write path.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m5.9 — Calculate cycles crossing linked open workbooks

**Type:** task · **Priority:** 3 · **Labels:** m5, ceng, all

**Blocking prerequisites:** `ceng-m5.2`, `ceng-m3.9`.
**Parent epic:** `ceng-m5`.

### Outcome
Calculate cycles crossing linked open workbooks. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Apply the same SCC/iteration execution to open-workbook links with captured application-level settings and initial state. Capture unresolved/conflicting setting cases and report explicit policy; do not solve them with cross-book rounds or stale link caches.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m5_9 -- --nocapture
cargo xtask accept --id ceng-m5.9 --platform windows --require-all
cargo xtask accept --id ceng-m5.9 --platform linux --require-all
cargo xtask accept --id ceng-m5.9 --platform wasm --require-all
```
- `ceng_m5_9__cross_book_cycle`: A retained two-book iterative witness calculates through one SCC and matches Excel observations.
- `ceng_m5_9__closed_link_transition`: Closing a linked workbook changes it to a versioned provider leaf with truthful provenance and no hidden iterative result reuse.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m5 — Complete CalcDoc — M5

**Type:** epic · **Priority:** 3 · **Labels:** m5, cdoc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcDoc in Iterative calculation, data tables, conditional formatting and validation have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 3 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cdoc-m5.1 — Model and author what-if data-table formulas

**Type:** task · **Priority:** 3 · **Labels:** m5, cdoc, all

**Blocking prerequisites:** `cdoc-m3.3`.
**Parent epic:** `cdoc-m5`.

### Outcome
Model and author what-if data-table formulas. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Read/write TABLE formula attributes, input-cell references, row/column orientation and table result blocks. Preserve the distinction from structured ListObject tables. Expose editable descriptors and relationships for host operations.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m5_1 -- --nocapture
cargo xtask accept --id cdoc-m5.1 --platform windows --require-all
cargo xtask accept --id cdoc-m5.1 --platform linux --require-all
cargo xtask accept --id cdoc-m5.1 --platform wasm --require-all
```
- `cdoc_m5_1__table_encoding`: One- and two-variable TABLE definitions round-trip with the same inputs and output region.
- `cdoc_m5_1__table_authoring`: A newly authored data table reopens without repair and Excel calculates the expected grid.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m5.2 — Model conditional-format rules and formula anchors

**Type:** task · **Priority:** 3 · **Labels:** m5, cdoc, all

**Blocking prerequisites:** `cdoc-m3.4`.
**Parent epic:** `cdoc-m5`.

### Outcome
Model conditional-format rules and formula anchors. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Read/write rule priorities, applies-to multi-areas, relative anchors, differential styles, stop-if-true and supported extension rule payloads. Author add/edit/remove/reorder operations while preserving unknown rule extensions unless a conflicting edit must refuse.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m5_2 -- --nocapture
cargo xtask accept --id cdoc-m5.2 --platform windows --require-all
cargo xtask accept --id cdoc-m5.2 --platform linux --require-all
cargo xtask accept --id cdoc-m5.2 --platform wasm --require-all
```
- `cdoc_m5_2__cf_roundtrip`: Formula and built-in conditional rules retain priorities, ranges and differential styles through edits.
- `cdoc_m5_2__relative_anchor`: Changing applies-to regions preserves the intended formula anchor semantics.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m5.3 — Model data-validation rules and input messages

**Type:** task · **Priority:** 3 · **Labels:** m5, cdoc, all

**Blocking prerequisites:** `cdoc-m3.4`.
**Parent epic:** `cdoc-m5`.

### Outcome
Model data-validation rules and input messages. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Read/write formula/list/whole/decimal/date/time/text-length/custom validation descriptors, applies-to ranges, blank policy, error styles and input messages. Preserve list separators/locale file syntax and extension payloads.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m5_3 -- --nocapture
cargo xtask accept --id cdoc-m5.3 --platform windows --require-all
cargo xtask accept --id cdoc-m5.3 --platform linux --require-all
cargo xtask accept --id cdoc-m5.3 --platform wasm --require-all
```
- `cdoc_m5_3__dv_roundtrip`: Inline-list and formula-based validation survive save/open with their messages and error styles.
- `cdoc_m5_3__validation_authoring`: New and deleted validation regions produce valid Excel XML and no orphan descriptors.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m5 — Complete CalcHost — M5

**Type:** epic · **Priority:** 3 · **Labels:** m5, chost, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcHost in Iterative calculation, data tables, conditional formatting and validation have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 3 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## chost-m5.1 — Expose iterative settings and what-if data-table commands

**Type:** task · **Priority:** 3 · **Labels:** m5, chost, all

**Blocking prerequisites:** `chost-m4.8`, `ceng-m5.3`, `ceng-m5.5`.
**Parent epic:** `chost-m5`.

### Outcome
Expose iterative settings and what-if data-table commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Implement iteration settings dialogs/commands and TABLE create/edit/remove commands with reversible authored patches. Show nonconvergence/cancellation and table-calculation state; never report refresh success before values are actually calculated.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m5_1 -- --nocapture
cargo xtask accept --id chost-m5.1 --platform windows --require-all
cargo xtask accept --id chost-m5.1 --platform linux --require-all
cargo xtask accept --id chost-m5.1 --platform wasm --require-all
```
- `chost_m5_1__table_command`: Creating a table through the command model changes the actual workbook and calculates its outputs.
- `chost_m5_1__iterative_receipt`: Changing iteration limits recalculates affected SCCs and exposes the real termination facts.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m5.2 — Implement CF rule authoring and projection commands

**Type:** task · **Priority:** 3 · **Labels:** m5, chost, all

**Blocking prerequisites:** `chost-m4.5`, `ceng-m5.7`.
**Parent epic:** `chost-m5`.

### Outcome
Implement CF rule authoring and projection commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Implement rule create/edit/delete/reorder/apply-to commands and style overlay subscriptions. Reuse ordinary formula editing assistance for rule formulas. Expose color-scale/bar/icon facts to every skin and persist only authored rules, not accidental view caches.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m5_2 -- --nocapture
cargo xtask accept --id chost-m5.2 --platform windows --require-all
cargo xtask accept --id chost-m5.2 --platform linux --require-all
cargo xtask accept --id chost-m5.2 --platform wasm --require-all
```
- `chost_m5_2__cf_command_roundtrip`: A rule authored through the command model affects visible cells and survives Excel reopen.
- `chost_m5_2__cf_undo`: Undo restores priorities/applicability and the calculated overlay deltas.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m5.3 — Implement validation authoring prompts and rejected edits

**Type:** task · **Priority:** 3 · **Labels:** m5, chost, all

**Blocking prerequisites:** `chost-m4.2`, `ceng-m5.8`.
**Parent epic:** `chost-m5`.

### Outcome
Implement validation authoring prompts and rejected edits. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Iterative calculation, data tables, conditional formatting and validation. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Implement DV editor commands, dropdown list projection, input messages, invalid-data indicators and context-aware prompt policies for typed entry versus paste/script paths. A rejected edit leaves both document and engine unchanged; accepted warnings record the actual authored write.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m5_3 -- --nocapture
cargo xtask accept --id chost-m5.3 --platform windows --require-all
cargo xtask accept --id chost-m5.3 --platform linux --require-all
cargo xtask accept --id chost-m5.3 --platform wasm --require-all
```
- `chost_m5_3__stop_is_atomic`: A stopped invalid entry changes neither workbook state nor undo history.
- `chost_m5_3__warning_override`: Accepting a warning commits exactly once, persists the rule and publishes the correct dependent values.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m6 — Complete Infrastructure — M6

**Type:** epic · **Priority:** 3 · **Labels:** m6, infra, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for Infrastructure in Excel core UI and complete core document authoring have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## infra-m6.1 — Prove complete core UI and document authoring

**Type:** task · **Priority:** 3 · **Labels:** m6, infra, oracle

**Blocking prerequisites:** `dnacalc-m6.24`, `onecalc-m6.1`, `cdoc-m6.7`, `infra-m3.1`, `infra-m1.2`, `cdoc-m6.1`, `cdoc-m6.2`, `cdoc-m6.3`, `cdoc-m6.4`, `cdoc-m6.5`, `cdoc-m6.6`, `cfml-m6.1`, `cfml-m6.2`, `chost-m6.1`, `chost-m6.2`, `chost-m6.3`, `chost-m6.4`, `dnacalc-m6.1`, `dnacalc-m6.2`, `dnacalc-m6.3`, `dnacalc-m6.4`, `dnacalc-m6.5`, `dnacalc-m6.6`, `dnacalc-m6.7`, `dnacalc-m6.8`, `dnacalc-m6.9`, `dnacalc-m6.10`, `dnacalc-m6.11`, `dnacalc-m6.12`, `dnacalc-m6.13`, `dnacalc-m6.14`, `dnacalc-m6.15`, `dnacalc-m6.16`, `dnacalc-m6.17`, `dnacalc-m6.18`, `dnacalc-m6.19`, `dnacalc-m6.20`, `dnacalc-m6.21`, `dnacalc-m6.22`, `dnacalc-m6.23`.
**Parent epic:** `infra-m6`.

### Outcome
Prove complete core UI and document authoring. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Execute every core command-family acceptance on the shipped app path, then save/reopen representative workbooks in Excel. Produce a parity ledger with test links; separable/phase-2 commands remain visibly pending and are not counted as core completion. Verify read/edit/write fidelity independently of the writer’s own ledger.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m6.1 --platform windows --require-all
cargo xtask accept --id infra-m6.1 --platform linux --require-all
cargo xtask accept --id infra-m6.1 --platform wasm --require-all
cargo xtask accept --id infra-m6.1 --platform excel --require-all
```
- `infra_m6_1__core_command_coverage`: Every core inventory row maps to an executed interaction plus headless command test; zero missing rows.
- `infra_m6_1__document_matrix`: Fresh and edited xlsx/xlsm core fixtures reopen in Excel without repair and untouched-part hashes match policy.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m6 — Complete CalcFml — M6

**Type:** epic · **Priority:** 3 · **Labels:** m6, cfml, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcFml in Excel core UI and complete core document authoring have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 2 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cfml-m6.1 — Complete number-format sections locales and numeric parsing

**Type:** task · **Priority:** 3 · **Labels:** m6, cfml, all

**Blocking prerequisites:** `cfml-m2.6`, `cfunc-m3.25`.
**Parent epic:** `cfml-m6`.

### Outcome
Complete number-format sections locales and numeric parsing. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Complete conditional/multi-section, scaling, fraction/scientific, elapsed-time, text, escape/padding and locale-dependent formats. Share the rounding/number-to-text policy with TEXT and entry parsing; preserve invariant serialization. Add compatibility-version and long-text boundaries.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/locale_format.rs: LocaleFormatContext; OxFml/crates/oxfml_core/src/format/ (resolve exact files in source manifest); OxFml/crates/oxfml_core/src/eval/mod.rs: locale use at function invocation. Do not move formatting into CalcEngine.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m6_1 -- --nocapture
cargo xtask accept --id cfml-m6.1 --platform windows --require-all
cargo xtask accept --id cfml-m6.1 --platform linux --require-all
cargo xtask accept --id cfml-m6.1 --platform wasm --require-all
```
- `cfml_m6_1__format_matrix`: All declared format grammar families have independent Excel-rendered string witnesses.
- `cfml_m6_1__format_value_seam`: TEXT and displayed cells agree on the same format/locale/date-system cases without two numeric-to-text implementations.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfml-m6.2 — Complete editor reference modes and diagnostics

**Type:** task · **Priority:** 3 · **Labels:** m6, cfml, all

**Blocking prerequisites:** `cfml-m2.5`, `cfml-m4.2`.
**Parent epic:** `cfml-m6`.

### Outcome
Complete editor reference modes and diagnostics. The behaviour is available through the CalcFml production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 5 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcFml files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_fml::compile and calc_fml::evaluate, or the public editing/formatting API consumed by both apps.

### Do
Implement point-mode range insertion, F4 cycling, reference colorization, name/table completion and locale separators using the same lossless source tree. Preserve IME composition and source-version cancellation at the app adapter boundary.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/syntax/ and OxFml/crates/oxfml_core/src/language_service/ are discovery paths. The source-manifest bead must resolve exact lexer/parser/service/test files and exported symbols before porting. Preserve lossless source spans; do not copy host-reference productions.

### Acceptance
```text
cargo test -p calc-fml --test acceptance cfml_m6_2 -- --nocapture
cargo xtask accept --id cfml-m6.2 --platform windows --require-all
cargo xtask accept --id cfml-m6.2 --platform linux --require-all
cargo xtask accept --id cfml-m6.2 --platform wasm --require-all
```
- `cfml_m6_2__reference_cycle`: Repeated F4 cycles relative/mixed/absolute references without modifying quoted strings or unrelated spans.
- `cfml_m6_2__incomplete_formula`: Diagnostics and completion remain stable for incomplete nested calls and non-ASCII names.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m6 — Complete CalcDoc — M6

**Type:** epic · **Priority:** 3 · **Labels:** m6, cdoc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcDoc in Excel core UI and complete core document authoring have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 7 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cdoc-m6.1 — Author complete cell formats and style tables

**Type:** task · **Priority:** 3 · **Labels:** m6, cdoc, all

**Blocking prerequisites:** `cdoc-m3.6`.
**Parent epic:** `cdoc-m6`.

### Outcome
Author complete cell formats and style tables. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Implement number/alignment/font/border/fill/protection records, differential formats, theme/tint references, merged-cell styles and style deduplication without renumbering untouched source IDs.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m6_1 -- --nocapture
cargo xtask accept --id cdoc-m6.1 --platform windows --require-all
cargo xtask accept --id cdoc-m6.1 --platform linux --require-all
cargo xtask accept --id cdoc-m6.1 --platform wasm --require-all
```
- `cdoc_m6_1__all_style_tabs`: Each Format Cells category survives independent XML inspection and Excel reopen.
- `cdoc_m6_1__style_dedup`: Equivalent newly authored styles share records while untouched original indices remain stable.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m6.2 — Author row column outline and pane metadata

**Type:** task · **Priority:** 3 · **Labels:** m6, cdoc, all

**Blocking prerequisites:** `cdoc-m3.6`.
**Parent epic:** `cdoc-m6`.

### Outcome
Author row column outline and pane metadata. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Write heights/widths, best-fit/custom flags, hidden rows/columns, outlines, freeze/split panes and sheet view zoom/selection with sparse default intervals.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m6_2 -- --nocapture
cargo xtask accept --id cdoc-m6.2 --platform windows --require-all
cargo xtask accept --id cdoc-m6.2 --platform linux --require-all
cargo xtask accept --id cdoc-m6.2 --platform wasm --require-all
```
- `cdoc_m6_2__pane_roundtrip`: Freeze and split panes plus hidden/outlined axes reopen in Excel with their authored state.
- `cdoc_m6_2__default_intervals`: Setting a million-row default does not allocate a million cell or style records.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m6.3 — Model comments notes and hyperlinks

**Type:** task · **Priority:** 3 · **Labels:** m6, cdoc, all

**Blocking prerequisites:** `cdoc-m3.5`.
**Parent epic:** `cdoc-m6`.

### Outcome
Model comments notes and hyperlinks. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Read/edit/write legacy notes, threaded comments/person relationships where supported, hyperlink targets/tooltips and drawing anchors. Preserve unedited unsupported comment extensions with a typed edit refusal when necessary.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m6_3 -- --nocapture
cargo xtask accept --id cdoc-m6.3 --platform windows --require-all
cargo xtask accept --id cdoc-m6.3 --platform linux --require-all
cargo xtask accept --id cdoc-m6.3 --platform wasm --require-all
```
- `cdoc_m6_3__comments_roundtrip`: Authored notes/comments and hyperlinks reopen without lost ownership or invalid relationships.
- `cdoc_m6_3__link_safety`: External hyperlinks are preserved as data and do not trigger network access on open.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m6.4 — Preserve macro projects signatures and package extensions

**Type:** task · **Priority:** 3 · **Labels:** m6, cdoc, all

**Blocking prerequisites:** `cdoc-m3.4`.
**Parent epic:** `cdoc-m6`.

### Outcome
Preserve macro projects signatures and package extensions. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Preserve vbaProject.bin, customXml, content types and package/application extension parts on safe edits. Detect signatures invalidated by edits and require an explicit refusal or acknowledged signature-removal action; never claim a modified signed package remains signed.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m6_4 -- --nocapture
cargo xtask accept --id cdoc-m6.4 --platform windows --require-all
cargo xtask accept --id cdoc-m6.4 --platform linux --require-all
cargo xtask accept --id cdoc-m6.4 --platform wasm --require-all
```
- `cdoc_m6_4__macro_preserved`: A value/formula edit leaves untouched vbaProject.bin hash-identical and never executes it.
- `cdoc_m6_4__signature_policy`: An edit invalidating a signature cannot silently retain a misleading valid-signature status.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m6.5 — Write calcPr calcChain and trustworthy formula caches

**Type:** task · **Priority:** 3 · **Labels:** m6, cdoc, all

**Blocking prerequisites:** `cdoc-m3.3`, `ceng-m4.3`.
**Parent epic:** `cdoc-m6`.

### Outcome
Write calcPr calcChain and trustworthy formula caches. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Serialize all calculation settings and cache provenance policy. Drop or regenerate stale calcChain only under the documented lawful policy; set recalc-on-open flags for stale/uncalculated results. Keep raw FileCached bytes when fidelity requires preservation, but never relabel them as newly calculated.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m6_5 -- --nocapture
cargo xtask accept --id cdoc-m6.5 --platform windows --require-all
cargo xtask accept --id cdoc-m6.5 --platform linux --require-all
cargo xtask accept --id cdoc-m6.5 --platform wasm --require-all
```
- `cdoc_m6_5__cache_provenance`: Saved caches correspond only to the recorded calculation generation or are explicitly stale with recalc policy.
- `cdoc_m6_5__calcchain_edit`: Structural edits do not leave a dangling calculation-chain cell or sheet reference.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m6.6 — Complete source-preserving package authoring and Save As

**Type:** task · **Priority:** 3 · **Labels:** m6, cdoc, all

**Blocking prerequisites:** `cdoc-m4.3`.
**Parent epic:** `cdoc-m6`.

### Outcome
Complete source-preserving package authoring and Save As. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Audit create/update/delete of all core part/relationship/content-type families, including empty/fresh workbooks and Save As. Stream untouched payloads and serialize changed regions with a fidelity ledger of copied, rewritten, normalized and refused parts.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m6_6 -- --nocapture
cargo xtask accept --id cdoc-m6.6 --platform windows --require-all
cargo xtask accept --id cdoc-m6.6 --platform linux --require-all
cargo xtask accept --id cdoc-m6.6 --platform wasm --require-all
```
- `cdoc_m6_6__fresh_core_workbook`: A source-free workbook with names, tables, styles, rules and formulas opens in Excel without repair.
- `cdoc_m6_6__ledger_matches_bytes`: An independent ZIP diff agrees with every fidelity-ledger copied/rewritten claim.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cdoc-m6.7 — Enforce transactional save output and lazy source access

**Type:** task · **Priority:** 3 · **Labels:** m6, cdoc, all

**Blocking prerequisites:** `cdoc-m6.6`.
**Parent epic:** `cdoc-m6`.

### Outcome
Enforce transactional save output and lazy source access. The behaviour is available through the CalcDoc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 7 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcDoc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_doc::open / prepare_edit / prepare_save / write; inspect the emitted package independently.

### Do
Prepare immutable save snapshots, write to temporary native output or a completed browser download/OPFS object, and acknowledge success only after output completion. Bound decoded XML retention and do not hold a second full authored cell table during save.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxDoc/crates/oxdoc-model/src/lib.rs (5,939 lines in one file: serde model types; 191 engine-named OxCalc* symbols such as OxCalcCell, OxCalcCellInput, OxCalcCellValue and OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/oxdoc-xlsx/src/lib.rs (37,648 lines in one file: OPC package reader/writer, source preservation and fidelity ledger, the RoundTripRefusal typed-refusal taxonomy near line 1204, WriteMode::FreshExport Save As, and the W010 tranche-2 authoring landed 2026-09-15: cell add, formula add/remove/change, sheet lifecycle); OxDoc/crates/oxdoc-conformance/ (10,934 lines of fixture-driven round-trip checks); OxDoc/docs/SPEC.md, DOCUMENT_LIFECYCLE_AND_HOST_BOUNDARIES.md and STRICT_LAZY_PARSING_SCOPE.md. The source exists locally (verified 2026-10-12) but has not been audited line by line: resolve the exact symbols in the infra-m0.4 manifest before copying, port behaviour (preservation, refusals, lazy profiles, authoring) rather than the monolithic file layout, and keep every OxCalc-named type out of the new crates.

### Acceptance
```text
cargo test -p calc-doc --test acceptance cdoc_m6_7 -- --nocapture
cargo xtask accept --id cdoc-m6.7 --platform windows --require-all
cargo xtask accept --id cdoc-m6.7 --platform linux --require-all
cargo xtask accept --id cdoc-m6.7 --platform wasm --require-all
```
- `cdoc_m6_7__failed_write`: An injected writer failure leaves the previous native file intact and never returns a successful browser-save receipt.
- `cdoc_m6_7__memory_accounting`: An allocation/ownership report for a 100MB fixture separates source bytes, parsed input, results and transient save buffers without duplicate authored cell stores.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m6 — Complete CalcHost — M6

**Type:** epic · **Priority:** 3 · **Labels:** m6, chost, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcHost in Excel core UI and complete core document authoring have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 4 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## chost-m6.1 — Implement panes dimensions grouping and sheet-view commands

**Type:** task · **Priority:** 3 · **Labels:** m6, chost, all

**Blocking prerequisites:** `chost-m4.5`, `cdoc-m6.2`.
**Parent epic:** `chost-m6`.

### Outcome
Implement panes dimensions grouping and sheet-view commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Implement freeze/split panes, row/column size and auto-fit, hide/unhide, outline/group/ungroup and zoom/view state through canonical commands. Auto-fit uses the shared text measurement service; view-only changes must not dirty unrelated formula nodes.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m6_1 -- --nocapture
cargo xtask accept --id chost-m6.1 --platform windows --require-all
cargo xtask accept --id chost-m6.1 --platform linux --require-all
cargo xtask accept --id chost-m6.1 --platform wasm --require-all
```
- `chost_m6_1__view_commands`: Headless commands and grid gestures produce equal authored/view patches.
- `chost_m6_1__no_recalc_on_zoom`: Zoom and pane movement issue no CalcFml evaluation calls.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m6.2 — Implement comments notes hyperlinks and review commands

**Type:** task · **Priority:** 3 · **Labels:** m6, chost, all

**Blocking prerequisites:** `chost-m4.3`, `cdoc-m6.3`.
**Parent epic:** `chost-m6`.

### Outcome
Implement comments notes hyperlinks and review commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Implement comments/notes lifecycle, hyperlink editing/follow policy and review/protection-related queries in one command family. Keep network navigation permission in platform adapters and expose editable-object projections to all skins.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m6_2 -- --nocapture
cargo xtask accept --id chost-m6.2 --platform windows --require-all
cargo xtask accept --id chost-m6.2 --platform linux --require-all
cargo xtask accept --id chost-m6.2 --platform wasm --require-all
```
- `chost_m6_2__comment_commands`: Create/edit/delete/undo a note or comment through HostState and reopen its saved package.
- `chost_m6_2__hyperlink_policy`: A workbook cannot cause automatic external navigation on load or recalc.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m6.3 — Implement formula audit trace and watch commands

**Type:** task · **Priority:** 3 · **Labels:** m6, chost, all

**Blocking prerequisites:** `chost-m4.12`, `ceng-m4.5`.
**Parent epic:** `chost-m6`.

### Outcome
Implement formula audit trace and watch commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Expose Evaluate Formula stepping, precedent/dependent queries, watch subscriptions, error-checking facts and selection aggregates. Use CalcFml trace policy plus engine read facts; do not build a second formula interpreter for the audit UI.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m6_3 -- --nocapture
cargo xtask accept --id chost-m6.3 --platform windows --require-all
cargo xtask accept --id chost-m6.3 --platform linux --require-all
cargo xtask accept --id chost-m6.3 --platform wasm --require-all
```
- `chost_m6_3__evaluate_formula`: Stepping a formula yields the same result and branch/provider calls as normal evaluation.
- `chost_m6_3__watch_delta`: A watch updates from value deltas rather than rereading its sheet or returning a stale unlabelled cache.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## chost-m6.4 — Implement document backstage and recovery commands

**Type:** task · **Priority:** 3 · **Labels:** m6, chost, all

**Blocking prerequisites:** `chost-m4.9`, `cdoc-m6.7`.
**Parent epic:** `chost-m6`.

### Outcome
Implement document backstage and recovery commands. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Implement new/open/close/save/save-as/recent/export, unsaved-change decisions and document recovery metadata in the application controller. Keep credentials/file dialogs in adapters; macro/connection trust remains explicit and disabled by default for untrusted sources.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m6_4 -- --nocapture
cargo xtask accept --id chost-m6.4 --platform windows --require-all
cargo xtask accept --id chost-m6.4 --platform linux --require-all
cargo xtask accept --id chost-m6.4 --platform wasm --require-all
```
- `chost_m6_4__unsaved_close`: Cancel/discard/save responses have distinct observable outcomes with no silent data loss.
- `chost_m6_4__recovery_replay`: A retained authored operation journal recovers the last committed state without claiming a crashed partial save succeeded.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## onecalc-m6 — Complete DNA OneCalc — M6

**Type:** epic · **Priority:** 3 · **Labels:** m6, onecalc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for DNA OneCalc in Excel core UI and complete core document authoring have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## onecalc-m6.1 — Complete OneCalc function exploration and fixture persistence

**Type:** task · **Priority:** 3 · **Labels:** m6, onecalc, all

**Blocking prerequisites:** `onecalc-m2.1`, `cfunc-m3.25`, `cfml-m6.2`, `cfunc-m4.1`, `cfml-m4.3`.
**Parent epic:** `onecalc-m6`.

### Outcome
Complete OneCalc function exploration and fixture persistence. The behaviour is available through the DNA OneCalc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA OneCalc files for this task; consume prerequisite APIs rather than editing another library. Production route: the OneCalc application controller with its in-memory reference provider, never CalcEngine.

### Do
Generate searchable function/operator help, arguments/examples, numeric mode/evidence status, trace and reference-provider fixtures from the single catalogue. Persist OneCalc session data independently of CalcHost/workbook operations; support import/export of formula test cases, not a hidden workbook engine.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/function.rs: FunctionMeta; crates/oxfunc_core/src/function_call.rs: FunctionCallTarget, FunctionCallScratch, FunctionExecutionContextBundle; crates/oxfunc_core/src/registry.rs; crates/oxfunc_core/src/functions/surface_dispatch.rs. Paths after the first are relative to OxFunc/.

### Acceptance
```text
cargo test -p dna-onecalc --test acceptance onecalc_m6_1 -- --nocapture
cargo xtask accept --id onecalc-m6.1 --platform windows --require-all
cargo xtask accept --id onecalc-m6.1 --platform linux --require-all
cargo xtask accept --id onecalc-m6.1 --platform wasm --require-all
```
- `onecalc_m6_1__catalogue_explorer`: Every migrated public function has generated help and an invocable fixture surface.
- `onecalc_m6_1__onecalc_boundary`: Native and browser dependency closures still exclude CalcEngine and CalcHost, including UI helper crates.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6 — Complete DNA Calc — M6

**Type:** epic · **Priority:** 0 · **Labels:** m6, dnacalc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for DNA Calc in Excel core UI and complete core document authoring have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 24 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## dnacalc-m6.1 — Build ribbon backstage quick access and key tips

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m2.5`, `infra-m3.1`, `chost-m6.4`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Build ribbon backstage quick access and key tips. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Render the complete captured ribbon/backstage/quick-access inventory from generated command metadata. Implement Alt key-tip state, contextual tab activation and customization persistence; use original or permissively licensed icons and retain asset provenance.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_1 -- --nocapture
cargo xtask accept --id dnacalc-m6.1 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.1 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.1 --platform wasm --require-all
```
- `dnacalc_m6_1__ribbon_commands`: Activating ribbon, quick-access and backstage entries invokes the correct canonical command.
- `dnacalc_m6_1__keytips`: Alt sequences navigate and execute without stealing editor text input.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.2 — Implement range multi-area and axis selection

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m2.5`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement range multi-area and axis selection. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Implement mouse/keyboard range selection, Ctrl/Shift extension, multi-area sets, entire row/column/sheet selection and selection anchors over virtualized geometry. Selection is view state; commands carry resolved areas.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_2 -- --nocapture
cargo xtask accept --id dnacalc-m6.2 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.2 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.2 --platform wasm --require-all
```
- `dnacalc_m6_2__selection_matrix`: Captured selection gestures produce expected selected areas and active cells.
- `dnacalc_m6_2__sparse_selection`: Selecting an entire worksheet does not enumerate its cells.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.3 — Implement edit enter point and IME modes

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.2`, `cfml-m6.2`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement edit enter point and IME modes. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Implement F2 edit versus enter mode, point-mode reference picking, F4 cycling, multi-line formula bar, input focus/commit/cancel and IME handling. Keep dirty editor-buffer undo separate from workbook undo.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_3 -- --nocapture
cargo xtask accept --id dnacalc-m6.3 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.3 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.3 --platform wasm --require-all
```
- `dnacalc_m6_3__mode_matrix`: Keyboard and pointer sequences match expected commits, cursor movement and reference insertion.
- `dnacalc_m6_3__ime_text`: Composition events commit once and do not issue premature workbook edits.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.4 — Implement fill handle and AutoFill series

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.2`, `chost-m4.2`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement fill handle and AutoFill series. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Add fill grip gestures, auto-fill options, copy versus series selection, dates/numbers/custom lists and repeated-formula preview. Execute one host fill command with correct reference adjustment and one undo group.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_4 -- --nocapture
cargo xtask accept --id dnacalc-m6.4 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.4 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.4 --platform wasm --require-all
```
- `dnacalc_m6_4__autofill_series`: Numeric/date and mixed-reference fill cases match independently captured workbook results.
- `dnacalc_m6_4__fill_undo`: Undo removes the complete fill and restores prior cells in one step.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.5 — Implement drag move copy and insert gestures

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.2`, `chost-m4.2`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement drag move copy and insert gestures. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Implement range drag with copy modifiers, move/insert destinations, conflict prompts and ghost previews. Use host preflight for merged/spill/protected targets; do not mutate the grid optimistically as a second truth.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_5 -- --nocapture
cargo xtask accept --id dnacalc-m6.5 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.5 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.5 --platform wasm --require-all
```
- `dnacalc_m6_5__drag_reference`: Moving a formula range updates dependent references using the host transform.
- `dnacalc_m6_5__drag_cancel`: Cancelling a conflict prompt changes no authored cell or undo history.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.6 — Implement clipboard and Paste Special UI

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.3`, `chost-m4.2`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement clipboard and Paste Special UI. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Support text/TSV/HTML clipboard interchange, an internal typed payload and publicly documented native Excel formats through platform adapters. Implement Paste Special dialogs, reference adjustment and Excel interop tests; unsupported browser-native formats are visible capabilities, not fake successes.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_6 -- --nocapture
cargo xtask accept --id dnacalc-m6.6 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.6 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.6 --platform wasm --require-all
```
- `dnacalc_m6_6__excel_clipboard`: Copy/paste to and from real Excel preserves declared values/formulas/formats for each supported format.
- `dnacalc_m6_6__paste_special_ui`: Dialog options reach the same tested headless commands and retain one undo group.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.7 — Implement freeze split and axis layout controls

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.2`, `chost-m6.1`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement freeze split and axis layout controls. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Expose freeze/split panes, row heights, column widths/auto-fit, hide/unhide and grouping/outline controls. Use logical segmented scroll positions to reach the full grid without relying on one browser element of enormous height.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_7 -- --nocapture
cargo xtask accept --id dnacalc-m6.7 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.7 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.7 --platform wasm --require-all
```
- `dnacalc_m6_7__panes_and_scroll`: Selections and edits remain on the correct cells across panes and at far-sheet positions.
- `dnacalc_m6_7__outline_visibility`: Grouping/hiding updates view geometry and visibility-dependent formula outputs.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.8 — Implement Format Cells Number and Alignment

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.3`, `chost-m4.5`, `cfml-m6.1`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement Format Cells Number and Alignment. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Implement Number/custom formats, locale previews, Alignment/orientation/wrap/shrink/merge controls using the shared format and text services. Preserve mixed-selection states and preview without committing.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_8 -- --nocapture
cargo xtask accept --id dnacalc-m6.8 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.8 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.8 --platform wasm --require-all
```
- `dnacalc_m6_8__format_dialog`: OK changes the actual styles and shared formatter output; Cancel changes neither.
- `dnacalc_m6_8__mixed_selection`: Mixed number/alignment values are represented explicitly rather than silently overwritten on dialog open.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.9 — Implement Format Cells Font Border Fill and Protection

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.8`, `chost-m4.7`, `cdoc-m6.1`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement Format Cells Font Border Fill and Protection. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Complete Font/Border/Fill/Protection tabs with previews, themes/tints, edge-specific borders, locking and hidden formulas. Apply one style patch and integrate accessible color controls.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_9 -- --nocapture
cargo xtask accept --id dnacalc-m6.9 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.9 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.9 --platform wasm --require-all
```
- `dnacalc_m6_9__remaining_format_tabs`: Each tab writes independently verified style records and round-trips through Excel.
- `dnacalc_m6_9__protection_effect`: Lock/hidden-formula settings alter command visibility/access under sheet protection as expected.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.10 — Implement cell styles themes and format painter

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.9`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement cell styles themes and format painter. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Expose styles gallery, theme changes, clear-formatting and format-painter interactions through shared style commands. Theme tokens remain distinct from resolved colors; selection previews do not mutate workbook styles.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_10 -- --nocapture
cargo xtask accept --id dnacalc-m6.10 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.10 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.10 --platform wasm --require-all
```
- `dnacalc_m6_10__theme_references`: Changing a theme updates theme-bound cells but not explicitly colored cells.
- `dnacalc_m6_10__format_painter`: Copying a style changes only formatting, not formulas or values.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.11 — Implement Name Manager and name-box navigation

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.3`, `chost-m4.3`, `cfml-m3.1`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement Name Manager and name-box navigation. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Implement scoped name list/create/edit/delete, refersto formula assistance, filters and error display; distinguish internal locator names from user names. Name-box selection uses the same typed reference service.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_11 -- --nocapture
cargo xtask accept --id dnacalc-m6.11 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.11 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.11 --platform wasm --require-all
```
- `dnacalc_m6_11__name_manager`: Editing a scoped name updates real dependent formulas and survives save/reopen.
- `dnacalc_m6_11__internal_locators`: Normal Name Manager use cannot accidentally rename hidden DNA locator names without an explicit advanced action.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.12 — Implement conditional-formatting UI and rule manager

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.9`, `chost-m5.2`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement conditional-formatting UI and rule manager. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Implement rule galleries, formula editing, priorities/stop-if-true, applies-to picking and rules manager. Render color scales/data bars/icon sets from engine facts over the actual grid and accessible projections.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_12 -- --nocapture
cargo xtask accept --id dnacalc-m6.12 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.12 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.12 --platform wasm --require-all
```
- `dnacalc_m6_12__cf_manager`: Reordering rules updates the correct visible style and survives Excel reopen.
- `dnacalc_m6_12__cf_not_fake`: Changing an input used by a rule updates pixels and accessible style facts through a real engine delta.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.13 — Implement data-validation dialog dropdowns and prompts

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.3`, `chost-m5.3`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement data-validation dialog dropdowns and prompts. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Implement the validation settings/input/error tabs, source range picking, dropdowns, messages and invalid-entry prompts. Match edit/paste context policy and allow explicit warning override.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_13 -- --nocapture
cargo xtask accept --id dnacalc-m6.13 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.13 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.13 --platform wasm --require-all
```
- `dnacalc_m6_13__dv_stop`: An invalid stopped entry retains the editor state and leaves the workbook unchanged.
- `dnacalc_m6_13__dv_list`: A formula-backed dropdown refreshes after its source changes and selecting an item issues a normal edit.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.14 — Implement Find Replace Go To and Go To Special

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.3`, `chost-m4.6`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement Find Replace Go To and Go To Special. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Expose find/replace options/results and Go To/Special dialogs with keyboard traversal, sparse result navigation and preview of replace-all. Invoke canonical query/edit commands with a single replace-all transaction.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_14 -- --nocapture
cargo xtask accept --id dnacalc-m6.14 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.14 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.14 --platform wasm --require-all
```
- `dnacalc_m6_14__find_replace_ui`: Formula/value scope and wildcard/case options reach the correct results and actual replacements.
- `dnacalc_m6_14__goto_special_ui`: Visible-only/error/formula selections are represented correctly on the grid.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.15 — Implement sort AutoFilter and advanced-filter UI

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.2`, `chost-m4.4`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement sort AutoFilter and advanced-filter UI. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Implement multi-key/custom sort dialogs, header filter menus, search/checklist/numeric/date/text filters and advanced-filter dialogs. Preserve selection and table context; changes use host row/visibility operations.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_15 -- --nocapture
cargo xtask accept --id dnacalc-m6.15 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.15 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.15 --platform wasm --require-all
```
- `dnacalc_m6_15__filter_ui`: Filtering affects actual row visibility and SUBTOTAL results rather than hiding pixels alone.
- `dnacalc_m6_15__sort_dialog`: Multi-key sort and undo produce the same workbook as the headless command tests.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.16 — Implement table creation and contextual table tools

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.15`, `chost-m4.3`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement table creation and contextual table tools. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Implement create/resize/rename/convert table, header/totals row options, calculated columns and table styles; show the contextual Table ribbon and structured-reference editing assistance.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_16 -- --nocapture
cargo xtask accept --id dnacalc-m6.16 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.16 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.16 --platform wasm --require-all
```
- `dnacalc_m6_16__table_tools`: A table created and edited from UI has real table parts and working structured formulas.
- `dnacalc_m6_16__table_convert`: Converting to a range preserves cell contents and adjusts dependent references correctly.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.17 — Implement insert delete cells rows columns and sheets UI

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.2`, `chost-m4.3`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement insert delete cells rows columns and sheets UI. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Expose context menus/dialogs and shortcuts for shifted-cell/axis/sheet insert/delete, sheet duplicate/move/hide/unhide and protection-aware prompts. Use the same preflight and undo commands as headless operations.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_17 -- --nocapture
cargo xtask accept --id dnacalc-m6.17 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.17 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.17 --platform wasm --require-all
```
- `dnacalc_m6_17__insert_delete_ui`: Reference transforms and persisted sheet state match the host command tests.
- `dnacalc_m6_17__protected_lifecycle`: Prohibited structural operations are blocked equally from menu and keyboard.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.18 — Implement calculation options and what-if table dialogs

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.3`, `chost-m5.1`, `chost-m4.8`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement calculation options and what-if table dialogs. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Expose automatic/manual/except-tables, F9 variants, iteration options, date/precision settings and Exact/Fast target/capability status. Add the actual Data Table dialog; reserve Goal Seek/Solver/other analytic commands for their area plan.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_18 -- --nocapture
cargo xtask accept --id dnacalc-m6.18 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.18 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.18 --platform wasm --require-all
```
- `dnacalc_m6_18__calculation_options`: Mode/settings changes reach real engine policy and visibly update provenance.
- `dnacalc_m6_18__table_dialog`: The dialog creates a real calculated data table rather than a static pasted grid.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.19 — Implement Evaluate Formula trace watch and error tools

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.3`, `chost-m6.3`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement Evaluate Formula trace watch and error tools. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Build Evaluate Formula stepping, Trace Precedents/Dependents arrows, Watch Window and Error Checking UI. Use shared trace/read facts; navigate multi-sheet/range precedents and label stale values.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_19 -- --nocapture
cargo xtask accept --id dnacalc-m6.19 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.19 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.19 --platform wasm --require-all
```
- `dnacalc_m6_19__formula_audit_ui`: Stepping and watch values equal normal production evaluation for the selected cell.
- `dnacalc_m6_19__trace_navigation`: Following a cross-sheet precedent opens the correct sheet/cell without synthesizing a separate expression evaluator.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.20 — Implement comments notes hyperlinks and protection UI

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.3`, `chost-m6.2`, `chost-m4.7`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement comments notes hyperlinks and protection UI. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Expose comments/notes editing, hyperlinks, protection dialogs and allowed-action lists with correct keyboard/focus behaviour. Display threaded/unsupported-preserved objects honestly rather than flattening ownership silently.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_20 -- --nocapture
cargo xtask accept --id dnacalc-m6.20 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.20 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.20 --platform wasm --require-all
```
- `dnacalc_m6_20__review_ui`: Notes/comments/protection changes persist and apply to commands in all skins.
- `dnacalc_m6_20__hyperlink_prompt`: External navigation requires the platform trust action and does not occur on hover or load.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.21 — Implement status aggregates zoom sheet tabs and window navigation

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.2`, `chost-m6.1`, `chost-m6.3`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Implement status aggregates zoom sheet tabs and window navigation. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Complete sheet-tab switching/reordering, status-bar aggregate selection, zoom controls, split/window navigation and accessibility summaries. Aggregate selections through host/provider services with sparse semantics, not screen-text parsing.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_21 -- --nocapture
cargo xtask accept --id dnacalc-m6.21 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.21 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.21 --platform wasm --require-all
```
- `dnacalc_m6_21__status_aggregates`: Displayed sum/count/average agree with typed selected values including blanks/errors.
- `dnacalc_m6_21__tabs_views`: Switching sheets retains the appropriate per-sheet view/selection state without altering workbook formulas.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.22 — Complete keyboard routing accessibility and command parity

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.1`, `dnacalc-m6.21`, `chost-m4.13`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Complete keyboard routing accessibility and command parity. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Reconcile all core shortcut/keytip/dialog commands with the captured inventory; implement a bounded accessible DOM mirror for the canvas grid, focus order and screen-reader announcements. Reserve Developer/IDE/forms/add-in slots with explicit phase-2 status and use the real panel/range-picker capability.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_22 -- --nocapture
cargo xtask accept --id dnacalc-m6.22 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.22 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.22 --platform wasm --require-all
```
- `dnacalc_m6_22__keyboard_parity`: Every core inventory shortcut executes its named command in the correct editor/selection context.
- `dnacalc_m6_22__screen_reader`: An actual accessibility-tree test can navigate/read/edit a cell and identify errors/spills without reading canvas pixels.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.23 — Complete non-Excel skin rules tables and protected editing

**Type:** task · **Priority:** 0 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m2.6`, `chost-m5.2`, `chost-m5.3`, `chost-m4.7`, `ceng-m3.4`, `ceng-m3.2`, `cfml-m3.4`, `chost-m4.2`, `dnacalc-m3.1`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Complete non-Excel skin rules tables and protected editing. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Expose validation/conditional-format effects, table membership, protected editing, structural changes and iterative/table-calculation diagnostics through the shared detail/command capabilities in notebook, tree and canvas. Preserve their distinct interactions without implementing a private calculation path.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-skin-ir/ and DnaTreeCalc/src/dnacalc-shell/src/stage.rs: StageSurface. The source-manifest bead resolves exact intent/projection symbols. Replace tree-first WorkspaceState and the monolithic intent enum.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_23 -- --nocapture
cargo xtask accept --id dnacalc-m6.23 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.23 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.23 --platform wasm --require-all
```
- `dnacalc_m6_23__all_skin_features`: The same rule/protection/table workbook exhibits the same values and accepted/rejected edits in all four skins.
- `dnacalc_m6_23__skin_switch_no_loss`: Switching and saving from each skin preserves the others’ layout/prose and canonical workbook data.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m6.24 — Test original visual assets browser limits and input interoperability

**Type:** task · **Priority:** 3 · **Labels:** m6, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.22`, `dnacalc-m6.23`.
**Parent epic:** `dnacalc-m6`.

### Outcome
Test original visual assets browser limits and input interoperability. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Excel core UI and complete core document authoring. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Record license/provenance for icons/fonts/illustrations, add pixel-level rendering checks alongside geometry/receipt tests, and test browser clipboard/key reservation/printing capability boundaries. Provide explicit alternate invocation routes when a browser intercepts a native shortcut; do not claim those limitations are exact native parity.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-stage-sheet/src/lib.rs: SheetStage and StageSurface wiring; src/dnacalc-stage-sheet/src/geometry.rs; src/dnacalc-stage-sheet/src/render_plan.rs; src/dnacalc-stage-sheet/src/selection.rs (relative to DnaTreeCalc). Carry pure geometry, not no-op interest or disabled core commands.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m6_24 -- --nocapture
cargo xtask accept --id dnacalc-m6.24 --platform windows --require-all
cargo xtask accept --id dnacalc-m6.24 --platform linux --require-all
cargo xtask accept --id dnacalc-m6.24 --platform wasm --require-all
```
- `dnacalc_m6_24__real_rendering`: Screenshot/visual tests catch missing canvas text/overlays even when debug counters are correct.
- `dnacalc_m6_24__platform_capabilities`: Unsupported native clipboard/key operations are exposed with tested alternatives and correctly marked parity status.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m7 — Complete Infrastructure — M7

**Type:** epic · **Priority:** 3 · **Labels:** m7, infra, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for Infrastructure in Conformance expansion, numerical campaigns and measured workloads have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 10 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## infra-m7.1 — Define licensed corpus manifests and stratified coverage

**Type:** task · **Priority:** 3 · **Labels:** m7, infra, tooling

**Blocking prerequisites:** `infra-m0.6`, `infra-m1.1`.
**Parent epic:** `infra-m7`.

### Outcome
Define licensed corpus manifests and stratified coverage. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Implement a content-addressed corpus manifest with source/license/redistribution disposition, acquisition hashes, Excel target, formula/participant counts and feature strata. Start with owner-authored or newly generated permissively licensed fixtures. Treat Enron, EUSES and FUSE as acquisition candidates, not automatically permissive collections; quarantine uncleared documents.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m7.1 --platform windows --require-all
cargo xtask accept --id infra-m7.1 --platform linux --require-all
```
- `infra_m7_1__license_gate`: A manifest entry without a recorded redistribution basis cannot enter the published corpus.
- `infra_m7_1__strata_counts`: Hidden sheets, names, arrays, rules, external links and multi-sheet cases are independently enumerated rather than counted from engine status flags.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m7.2 — Generate and capture cross-feature workbook families

**Type:** task · **Priority:** 3 · **Labels:** m7, infra, oracle

**Blocking prerequisites:** `infra-m7.1`, `infra-m1.2`, `infra-m5.3`.
**Parent epic:** `infra-m7`.

### Outcome
Generate and capture cross-feature workbook families. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Build parameterized Excel-authored workbook generators for scope shadowing, mixed refs, sheet edits, arrays/spills, names/LAMBDAs, dates/locales, iteration, TABLE, CF and DV. Retain at least 1,000 distinct workbooks, including 200 multi-sheet and 50 containing three or more interacting feature categories. These are evidence-volume floors, not runtime targets; report generator independence and minimize failures.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m7.2 --platform windows --require-all
cargo xtask accept --id infra-m7.2 --platform linux --require-all
cargo xtask accept --id infra-m7.2 --platform wasm --require-all
cargo xtask accept --id infra-m7.2 --platform excel --require-all
```
- `infra_m7_2__corpus_floor`: Independent manifest inspection proves the stated workbook/stratum counts without counting byte-identical copies.
- `infra_m7_2__reproducible_capture`: A sampled case regenerates its authored inputs and records fresh Excel results through the one capture service.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m7.3 — Acquire public corpus candidates with provenance review

**Type:** task · **Priority:** 3 · **Labels:** m7, infra, tooling

**Blocking prerequisites:** `infra-m7.1`.
**Parent epic:** `infra-m7`.

### Outcome
Acquire public corpus candidates with provenance review. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Inventory the original Enron/EUSES/FUSE distribution sources, record collection and individual-file rights, privacy and security constraints, and acquire only permitted fixtures. Maintain a private quarantined evaluation lane where permitted but not redistributable; do not bundle such workbooks. Emit a retained admission report even when no external collection qualifies.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh

### Acceptance
```text
cargo xtask accept --id infra-m7.3 --platform windows --require-all
cargo xtask accept --id infra-m7.3 --platform linux --require-all
```
- `infra_m7_3__admission_report`: Every admitted file has its original source, content hash and explicit rights disposition; rejected candidates remain visible in the report.
- `infra_m7_3__privacy_safety`: Macros, credentials, external connections and potentially sensitive content are not executed or published by corpus ingestion.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Do not claim that public availability is a permissive license. Corpus access does not authorize redistribution or macro execution.

---

## infra-m7.4 — Unify live and cached verdicts with raw numeric observations

**Type:** task · **Priority:** 3 · **Labels:** m7, infra, oracle

**Blocking prerequisites:** `infra-m7.2`, `infra-m1.1`.
**Parent epic:** `infra-m7`.

### Outcome
Unify live and cached verdicts with raw numeric observations. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Extend the single verdict path with raw Value2 binary64 observations, exact error/type/array shape, saved XML cache text and freshness/Excel-build evidence. Keep cached-XML-only matches distinct from bit-certified raw observations; inspect formula inventories independently. Emit calculated/formula counts first, then matched/calculated and unsupported/stale/failure counts.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m7.4 --platform windows --require-all
cargo xtask accept --id infra-m7.4 --platform linux --require-all
cargo xtask accept --id infra-m7.4 --platform wasm --require-all
cargo xtask accept --id infra-m7.4 --platform excel --require-all
```
- `infra_m7_4__cached_not_pass`: A formula whose input cache matches Excel but which was never evaluated contributes zero calculated/matched cells.
- `infra_m7_4__raw_bits`: Two adjacent binary64 values that serialize ambiguously in an XML cache are distinguished by retained raw observations.
- `infra_m7_4__population_reconciles`: Every formula coordinate has exactly one provenance/verdict category, including errors, spills and not-modeled cells.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m7.5 — Port one black-box smart-fuzzer campaign runner

**Type:** task · **Priority:** 3 · **Labels:** m7, infra, oracle

**Blocking prerequisites:** `infra-m7.4`, `infra-m0.5`, `cfunc-m3.25`.
**Parent epic:** `infra-m7`.

### Outcome
Port one black-box smart-fuzzer campaign runner. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Retain one search/capture/replay pipeline for generated function inputs and calculation-graph candidates, using only public documentation and black-box Excel observations. Minimize counterexamples; retain all mismatches, crashes and target fingerprints; never train a literal lookup table into the shipping evaluator. Emit a campaign manifest that permits interrupted runs to resume without losing witnesses.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m7.5 --platform windows --require-all
cargo xtask accept --id infra-m7.5 --platform linux --require-all
cargo xtask accept --id infra-m7.5 --platform wasm --require-all
cargo xtask accept --id infra-m7.5 --platform excel --require-all
```
- `infra_m7_5__failure_retention`: An injected mismatch is minimized, saved and reproduced by the same verdict path on all portable targets.
- `infra_m7_5__no_answer_table`: Candidate promotion requires a general implementation plus held-out witnesses; memorized expected-value branches fail the production-source gate.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m7.6 — Measure native target variation and software-Exact feasibility

**Type:** task · **Priority:** 3 · **Labels:** m7, infra, oracle

**Blocking prerequisites:** `cfunc-m7.1`, `cfunc-m3.24`, `infra-m7.4`.
**Parent epic:** `infra-m7`.

### Outcome
Measure native target variation and software-Exact feasibility. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Capture held-out EXP/LN/LOG/POWER and midpoint-sensitive witnesses on available distinct x86-64 CPU families and Excel builds; run the portable candidate in actual wasm. Retain mismatches, CPU fingerprints, backend costs and missing-target coverage. Produce a decision artifact separating ext80 arithmetic agreement from transcendental algorithm/microcode agreement; unavailable machines are explicit missing evidence, not inferred parity.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs: exp, pow_direct, atan and the fyl2x_fn expansion are QUARANTINED, not port sources until provenance clearance. OxFunc/crates/oxfunc_core/src/excel_numeric/ is a discovery location only. Use independent public-ISA/black-box witnesses for replacement code.

### Acceptance
```text
cargo xtask accept --id infra-m7.6 --platform windows --require-all
cargo xtask accept --id infra-m7.6 --platform linux --require-all
cargo xtask accept --id infra-m7.6 --platform wasm --require-all
cargo xtask accept --id infra-m7.6 --platform excel --require-all
```
- `infra_m7_6__target_matrix`: Each tested CPU/build/backend pair has independent observed bits and a reproducible mismatch count.
- `infra_m7_6__wasm_feasibility`: Actual browser runs record correctness and cost for the portable candidate without relabeling its mismatches as Exact passes.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m7.7 — Verify portable Fast determinism and numeric mode defaults

**Type:** task · **Priority:** 3 · **Labels:** m7, infra, oracle

**Blocking prerequisites:** `cfunc-m7.2`, `chost-m4.8`, `onecalc-m6.1`.
**Parent epic:** `infra-m7`.

### Outcome
Verify portable Fast determinism and numeric mode defaults. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Replay identical effect tapes on Windows, Linux and actual wasm with pinned portable math and numeric parsing/text policies. Verify Fast is the default for newly created workbooks and OneCalc sessions on all platforms, and Exact requires an explicitly available target. A persisted Exact request on an unsupported target prompts a choice before calculation; it never silently becomes Fast.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m7.7 --platform windows --require-all
cargo xtask accept --id infra-m7.7 --platform linux --require-all
cargo xtask accept --id infra-m7.7 --platform wasm --require-all
cargo xtask accept --id infra-m7.7 --platform excel --require-all
```
- `infra_m7_7__fast_cross_target`: All deterministic Fast witnesses produce the same values/types/shapes on the tested target matrix; unresolved differences block the deterministic claim.
- `infra_m7_7__mode_reopen`: Opening Exact metadata in wasm exposes NumericModeUnavailable and leaves prior caches clearly stale until a user chooses an available mode.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows with an installed, isolated Excel instance for live capture. The capture artifact and verdict replay run on Windows, Linux and wasm. Missing Excel is reported as not run, never a pass.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m7.8 — Record named product-path benchmarks without thresholds

**Type:** task · **Priority:** 3 · **Labels:** m7, infra, tooling

**Blocking prerequisites:** `infra-m6.1`, `infra-m7.7`, `ceng-m7.1`.
**Parent epic:** `infra-m7`.

### Outcome
Record named product-path benchmarks without thresholds. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Add named cold/warm recalc, 100k fill-down sharing, 20k chains, cross-sheet edits, whole-column ranges, 100k-plus materialization, 1M-row viewport, 10k notebook, 100MB workbook-memory, SCC/TABLE/CF/DV and numeric-backend workloads. Record platform/build/mode, timings, allocations, peak memory and structural counters. The browser runs actual wasm; report compressed input versus decoded size separately. Set no speed, memory or duration targets.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo xtask accept --id infra-m7.8 --platform windows --require-all
cargo xtask accept --id infra-m7.8 --platform linux --require-all
```
- `infra_m7_8__benchmark_artifact`: Each named workload has a retained raw measurement and correctness verdict, or an explicit not-run reason.
- `infra_m7_8__structural_counter`: An unrelated one-cell edit has no whole-sheet clone/readout in independent allocation/read instrumentation; display delta size reflects changed interests rather than sheet size.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m7.9 — Fuzz operation sequences package parsing and cancellation

**Type:** task · **Priority:** 3 · **Labels:** m7, infra, all

**Blocking prerequisites:** `infra-m6.1`, `ceng-m7.1`.
**Parent epic:** `infra-m7`.

### Outcome
Fuzz operation sequences package parsing and cancellation. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Generate bounded operation sequences across edits, insert/delete, names, mode switches, spills, undo/redo, save/open and cancellation. Add malformed ZIP/XML/relationship inputs with resource-limit classification. Compare snapshots, deltas and saved packages after each committed operation; retain minimized crash/semantic witnesses.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo xtask accept --id infra-m7.9 --platform windows --require-all
cargo xtask accept --id infra-m7.9 --platform linux --require-all
cargo xtask accept --id infra-m7.9 --platform wasm --require-all
```
- `infra_m7_9__operation_replay`: Replaying a minimized sequence yields equal authored states and independently computed reference results at every checkpoint.
- `infra_m7_9__hostile_package`: ZIP bombs, path traversal and malformed relationships produce bounded typed failures without external resource access.
- `infra_m7_9__cancel_consistency`: Cancellation never publishes a partially calculated generation as fresh.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m7.10 — Publish conformance and campaign evidence through one report

**Type:** task · **Priority:** 3 · **Labels:** m7, infra, tooling

**Blocking prerequisites:** `infra-m7.4`, `infra-m7.5`, `infra-m7.7`, `infra-m7.8`, `infra-m7.9`, `infra-m7.3`, `infra-m7.6`.
**Parent epic:** `infra-m7`.

### Outcome
Publish conformance and campaign evidence through one report. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Generate one report from immutable manifests: catalogue scope, calculated/formula coverage, numeric target/mode, exact and Fast verdicts, feature intersections, unsupported/refused/not-run populations and benchmark links. Include known mismatches rather than hiding them in waived totals; cross-check claimed coverage against independently enumerated source workbooks and tests.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m7.10 --platform windows --require-all
cargo xtask accept --id infra-m7.10 --platform linux --require-all
```
- `infra_m7_10__report_reconciliation`: All headline totals reconcile to immutable case manifests and are reproducible from a clean checkout.
- `infra_m7_10__honesty_regression`: Injected cached-only, unsupported, skipped and divergent cases cannot increase calculated-match coverage or disappear from totals.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## cfunc-m7 — Complete CalcFunc — M7

**Type:** epic · **Priority:** 3 · **Labels:** m7, cfunc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcFunc in Conformance expansion, numerical campaigns and measured workloads have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 2 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## cfunc-m7.1 — Measure a portable extended-precision arithmetic candidate

**Type:** task · **Priority:** 3 · **Labels:** m7, cfunc, all

**Blocking prerequisites:** `cfunc-m3.22`, `infra-m0.5`.
**Parent epic:** `cfunc-m7`.

### Outcome
Measure a portable extended-precision arithmetic candidate. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
In CalcFunc numeric research tooling, implement or bind a provenance-cleared ext80 arithmetic candidate for add/subtract/multiply/divide/conversion and explicit rounding. Cross-check against independently captured x87 primitive witnesses. Do not advertise Excel-equivalent transcendental functions merely because ext80 arithmetic works; keep this research backend out of the shipping Exact capability table.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs: exp, pow_direct, atan and the fyl2x_fn expansion are QUARANTINED, not port sources until provenance clearance. OxFunc/crates/oxfunc_core/src/excel_numeric/ is a discovery location only. Use independent public-ISA/black-box witnesses for replacement code.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m7_1 -- --nocapture
cargo xtask accept --id cfunc-m7.1 --platform windows --require-all
cargo xtask accept --id cfunc-m7.1 --platform linux --require-all
cargo xtask accept --id cfunc-m7.1 --platform wasm --require-all
```
- `cfunc_m7_1__ext80_primitives`: Independent primitive witnesses distinguish intermediate extended rounding from final binary64 rounding.
- `cfunc_m7_1__no_false_exact`: The portable candidate is not selectable as a certified Exact backend without the separate target/algorithm evidence required by numeric policy.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
No software Exact product commitment and no formal proofs. Transcendental model identification and CPU-family equivalence remain independent evidence questions.

---

## cfunc-m7.2 — Declare measured Fast-mode tolerance contracts

**Type:** task · **Priority:** 3 · **Labels:** m7, cfunc, all

**Blocking prerequisites:** `cfunc-m3.25`, `infra-m7.4`.
**Parent epic:** `cfunc-m7`.

### Outcome
Declare measured Fast-mode tolerance contracts. The behaviour is available through the CalcFunc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 5, 10 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only CalcFunc files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_func::invoke through the generated catalogue; do not call a detached test implementation.

### Do
Create per-function/operator absolute/relative/ULP acceptance policies with domain and conditioning notes, derived from held-out Exact-target observations. Errors, booleans, text and shapes remain exact semantic comparisons. Distinguish numerical deviations propagated through branch decisions from harmless output distance; keep uncharacterized families visibly uncharacterized rather than giving one global epsilon.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/crates/oxfunc_core/src/function.rs: FunctionMeta; crates/oxfunc_core/src/function_call.rs: FunctionCallTarget, FunctionCallScratch, FunctionExecutionContextBundle; crates/oxfunc_core/src/registry.rs; crates/oxfunc_core/src/functions/surface_dispatch.rs. Paths after the first are relative to OxFunc/.

### Acceptance
```text
cargo test -p calc-func --test acceptance cfunc_m7_2 -- --nocapture
cargo xtask accept --id cfunc-m7.2 --platform windows --require-all
cargo xtask accept --id cfunc-m7.2 --platform linux --require-all
cargo xtask accept --id cfunc-m7.2 --platform wasm --require-all
```
- `cfunc_m7_2__tolerance_metadata`: Every numeric-sensitive callable has an explicit policy or an uncharacterized status; generated oracle paths use that one policy.
- `cfunc_m7_2__branch_drift`: A small numeric difference that changes IF/comparison/error/shape behaviour is reported as a semantic divergence even when an output epsilon would hide it.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## ceng-m7 — Complete CalcEngine — M7

**Type:** epic · **Priority:** 3 · **Labels:** m7, ceng, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcEngine in Conformance expansion, numerical campaigns and measured workloads have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## ceng-m7.1 — Retain the reference strategy and differential-twin harness

**Type:** task · **Priority:** 3 · **Labels:** m7, ceng, all

**Blocking prerequisites:** `ceng-m5.9`, `ceng-m4.5`.
**Parent epic:** `ceng-m7`.

### Outcome
Retain the reference strategy and differential-twin harness. The behaviour is available through the CalcEngine production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 6 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Conformance expansion, numerical campaigns and measured workloads. Own only CalcEngine files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_engine::prepare_edit / begin_recalc / step / finish; use the production CalcFml evaluator.

### Do
Expose the permanent single-threaded reference strategy behind the calculation API and implement a differential harness that can consume a future optimized strategy. Compare evaluated nodes, dependency/read footprints, spill ownership, provenance and values under the same operations/effect tape. Test an intentionally faulty strategy in test tooling; do not introduce a second shipping graph or formula evaluator now.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; OxCalc/src/oxcalc-core/src/grid/machine/workbook_nodes.rs: recalc tick/RNG design; OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.

### Acceptance
```text
cargo test -p calc-engine --test acceptance ceng_m7_1 -- --nocapture
cargo xtask accept --id ceng-m7.1 --platform windows --require-all
cargo xtask accept --id ceng-m7.1 --platform linux --require-all
cargo xtask accept --id ceng-m7.1 --platform wasm --require-all
```
- `ceng_m7_1__faulty_twin_detected`: A strategy with a missed dynamic dependency fails comparison even when initial cell values match.
- `ceng_m7_1__same_evaluator`: Architecture gates prove both strategy test adapters call CalcFml rather than independent expression implementations.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m8 — Complete Infrastructure — M8

**Type:** epic · **Priority:** 4 · **Labels:** m8, infra, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for Infrastructure in Release integration and first-version scope accounting have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 2 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## infra-m8.1 — Verify core completion and mandatory phase-2 attachments

**Type:** task · **Priority:** 4 · **Labels:** m8, infra, tooling

**Blocking prerequisites:** `infra-m7.10`, `dnacalc-m8.1`, `onecalc-m8.1`, `chost-m4.13`, `cfunc-m4.1`, `cfml-m4.3`, `ceng-m4.6`, `infra-m0.1`, `infra-m0.2`, `infra-m0.3`, `cfunc-m0.1`, `cdoc-m0.1`, `infra-m1.3`, `cfml-m2.1`, `cfml-m2.2`, `ceng-m2.1`, `onecalc-m2.1`, `infra-m2.1`, `infra-m3.2`, `infra-m3.3`, `ceng-m3.8`, `chost-m3.2`, `infra-m4.1`, `chost-m4.10`, `chost-m4.11`, `chost-m4.12`, `infra-m5.3`, `infra-m6.1`.
**Parent epic:** `infra-m8`.

### Outcome
Verify core completion and mandatory phase-2 attachments. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Release integration and first-version scope accounting. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Reconcile this plan’s core scope, all UI-core rows, platform acceptance and every phase-2 attachment with retained tests and non-test call sites. Mark this checkpoint CORE COMPLETE only; chart/pivot/print and other separable implementations must remain pending until their generated task graphs actually land. Fail on missing runtime callers and stale self-reported completion flags.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict precedent. Resolve and record exact files in infra-m0.4; retain only one new COM capture implementation and one verdict implementation.

### Acceptance
```text
cargo xtask accept --id infra-m8.1 --platform windows --require-all
cargo xtask accept --id infra-m8.1 --platform linux --require-all
```
- `infra_m8_1__core_matrix`: Every core requirement maps to executed retained evidence on its declared platforms.
- `infra_m8_1__seam_traffic`: UDF registration/invalidation, late-bound dispatch, events, exclusive transactions, external/RTD and callback/range/panel attachments are invoked by actual application or command-line controllers, not test-only code.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## infra-m8.2 — Enforce first-version release scope including separable areas

**Type:** task · **Priority:** 4 · **Labels:** m8, infra, tooling

**Blocking prerequisites:** `infra-m8.1`, `infra-m3.20`, `infra-m3.21`, `infra-m3.22`, `infra-m3.23`, `infra-m3.24`.
**Parent epic:** `infra-m8`.

### Outcome
Enforce first-version release scope including separable areas. The behaviour is available through the Infrastructure production path described below, with the named acceptance cases passing.

### Context
Implements §§1, 10–13 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Release integration and first-version scope accounting. Own only Infrastructure files for this task; consume prerequisite APIs rather than editing another library. Production route: cargo xtask / calcctl; call the same exported library entrypoints as the applications.

### Do
Implement a fail-closed release-scope checker that discovers the implementation graphs generated by each separable-area planning bead and verifies their executable exits. Initially it MUST fail because those areas are not built. Publish a core-ready build separately; a first-version release requires actual chart rendering, pivot refresh/GETPIVOTDATA, printing/layout and all inventoried phase-1 commands. Preserve explicit phase-2 exclusions without quietly moving phase-1 features there.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
fresh

### Acceptance
```text
cargo xtask accept --id infra-m8.2 --platform windows --require-all
cargo xtask accept --id infra-m8.2 --platform linux --require-all
```
- `infra_m8_2__planning_is_not_shipping`: With every planning bead closed but no chart/pivot/print implementation evidence, the release checker still fails.
- `infra_m8_2__actual_release`: Only retained shipped-product tests for every mandatory area allow the checker to pass; unknown inventory rows or missing child graphs block release.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux for repository/tooling execution. Artifacts produced here are consumed by wasm/browser CI; this bead does not imply running a native CLI inside wasm.

### Out of scope
This task builds the release gate, not the separable implementations. Closing this gate-implementation task must not itself mark the first-version release milestone complete.

---

## chost-m8 — Complete CalcHost — M8

**Type:** epic · **Priority:** 4 · **Labels:** m8, chost, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for CalcHost in Release integration and first-version scope accounting have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## chost-m8.1 — Complete application lifecycle and cross-skin recovery

**Type:** task · **Priority:** 4 · **Labels:** m8, chost, all

**Blocking prerequisites:** `chost-m6.4`, `chost-m4.13`, `dnacalc-m6.23`.
**Parent epic:** `chost-m8`.

### Outcome
Complete application lifecycle and cross-skin recovery. The behaviour is available through the CalcHost production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 3, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Release integration and first-version scope accounting. Own only CalcHost files for this task; consume prerequisite APIs rather than editing another library. Production route: calc_host::dispatch / step / project / save through a real HostState, not a parallel session.

### Do
Finish the host lifecycle controller for multiple documents, unsaved recovery, exclusive batch mode, external update ordering and per-skin interests. Run the platform-neutral controller without a UI, and consume it unchanged through desktop and browser transports. Preserve explicit capability failures and saved numeric preferences.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession, XlsxSourceState; DnaTreeCalc/src/dnacalc-host-core/src/command.rs: HostCommand; src/dnacalc-host-core/src/grid_publication.rs (relative to DnaTreeCalc). Use these as behavioural references only; replace engine-owned authoring.

### Acceptance
```text
cargo test -p calc-host --test acceptance chost_m8_1 -- --nocapture
cargo xtask accept --id chost-m8.1 --platform windows --require-all
cargo xtask accept --id chost-m8.1 --platform linux --require-all
cargo xtask accept --id chost-m8.1 --platform wasm --require-all
```
- `chost_m8_1__lifecycle_recovery`: Crash/recovery and reopen across two skins preserve the last committed authored revision without duplicate operations.
- `chost_m8_1__serial_updates`: An external update queued during an exclusive transaction becomes one ordered operation after commit and produces matching graph/document revisions.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## onecalc-m8 — Complete DNA OneCalc — M8

**Type:** epic · **Priority:** 4 · **Labels:** m8, onecalc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for DNA OneCalc in Release integration and first-version scope accounting have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## onecalc-m8.1 — Verify the shipped OneCalc package stays engine-free

**Type:** task · **Priority:** 4 · **Labels:** m8, onecalc, all

**Blocking prerequisites:** `onecalc-m6.1`, `infra-m7.7`.
**Parent epic:** `onecalc-m8`.

### Outcome
Verify the shipped OneCalc package stays engine-free. The behaviour is available through the DNA OneCalc production path described below, with the named acceptance cases passing.

### Context
Implements §§2, 4, 8 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Release integration and first-version scope accounting. Own only DNA OneCalc files for this task; consume prerequisite APIs rather than editing another library. Production route: the OneCalc application controller with its in-memory reference provider, never CalcEngine.

### Do
Package Windows/Linux and browser OneCalc, exercise actual formula editing/binding/evaluation/formatting and numeric-mode behaviour, and inspect the complete resolved dependency closure. Document session fixture interchange separately from workbook loading; do not add CalcHost to get a convenient file dialog or provider.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
OxFml/crates/oxfml_core/src/eval/mod.rs: compiled expressions and evaluation; OxFml/crates/oxfml_core/src/binding/profile.rs: ReferenceNormalFormKey, ReferenceFingerprintPolicy. Discard ReferencePolicy, host-reference syntax and the tree profile.

### Acceptance
```text
cargo test -p dna-onecalc --test acceptance onecalc_m8_1 -- --nocapture
cargo xtask accept --id onecalc-m8.1 --platform windows --require-all
cargo xtask accept --id onecalc-m8.1 --platform linux --require-all
cargo xtask accept --id onecalc-m8.1 --platform wasm --require-all
```
- `onecalc_m8_1__shipped_onecalc`: The distributed app evaluates typed reference fixtures and LAMBDA/array cases using only CalcFml and CalcFunc.
- `onecalc_m8_1__engine_free`: The complete native/web application dependency graphs contain no CalcEngine or CalcHost crate.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

## dnacalc-m8 — Complete DNA Calc — M8

**Type:** epic · **Priority:** 4 · **Labels:** m8, dnacalc, epic

**Blocking prerequisites:** none.
**Parent epic:** none.

### Outcome
All child tasks for DNA Calc in Release integration and first-version scope accounting have their required observable acceptance evidence.

### Context
Grouping epic for the implementation plan, not an implementation task or a release certification.

### Do
Track child tasks only. Own no implementation work; do not make children wait for this epic to close.

### Port from
Not applicable; exact source/admission instructions are in each child task.

### Acceptance
Inspect the executed evidence of all 1 child tasks. Epic closure is bookkeeping, not independent proof of product behaviour. A later separable-area implementation is not complete merely because its planning task closes.

### Platforms
The union of the child tasks' declared target matrices.

### Out of scope
Additional implementation work or silently changing the owner's scope.

---

## dnacalc-m8.1 — Integrate all skins and commands in the shipped packages

**Type:** task · **Priority:** 4 · **Labels:** m8, dnacalc, all

**Blocking prerequisites:** `dnacalc-m6.24`, `chost-m8.1`, `infra-m7.10`.
**Parent epic:** `dnacalc-m8`.

### Outcome
Integrate all skins and commands in the shipped packages. The behaviour is available through the DNA Calc production path described below, with the named acceptance cases passing.

### Context
Implements §§8, 9 of DNA_CALC_REIMPLEMENTATION_PLAN.md, within Release integration and first-version scope accounting. Own only DNA Calc files for this task; consume prerequisite APIs rather than editing another library. Production route: the shipped skin controller and CalcHost transport; browser tests dispatch real UI events.

### Do
Package the desktop and browser DNA Calc applications using the same application/skin controllers. Execute cross-skin editing, dependent recalc, save/reopen and accessibility scenarios through the actual distributed entrypoints. Make unavailable platform features explicit; do not substitute a test-only host for end-to-end tests.
Add the named acceptance tests to this package's acceptance suite and register their exact names in its local acceptance manifest. Keep imported-source blob IDs and any changed semantic witnesses in the migration record. All proposed commands become real repository commands through infra-m0.3; a zero-test run is a failure.

### Port from
DnaTreeCalc/src/dnacalc-skin-ir/ and DnaTreeCalc/src/dnacalc-shell/src/stage.rs: StageSurface. The source-manifest bead resolves exact intent/projection symbols. Replace tree-first WorkspaceState and the monolithic intent enum.

### Acceptance
```text
cargo test -p dna-calc --test acceptance dnacalc_m8_1 -- --nocapture
cargo xtask accept --id dnacalc-m8.1 --platform windows --require-all
cargo xtask accept --id dnacalc-m8.1 --platform linux --require-all
cargo xtask accept --id dnacalc-m8.1 --platform wasm --require-all
```
- `dnacalc_m8_1__shipped_entrypoints`: Installed Windows/Linux builds and the deployed browser bundle execute the same retained scenario with identical authored semantics.
- `dnacalc_m8_1__switch_and_save`: Notebook/tree/canvas/Excel edits coexist across save and real-Excel reopen; missing manifest metadata causes diagnosed flattening, not silent value reconstruction.
The acceptance runner must verify every named case actually executed and retain the machine-readable receipt. Assertions compare observable outputs, operation effects or independently instrumented calls, not a success flag assigned by the implementation itself.

### Platforms
Windows and Linux native tests, plus actual wasm browser execution in Chromium and Firefox. Run the same semantic fixtures on every target; platform adapters also have their native integration tests.

### Out of scope
Other command/function families and performance tuning. Do not add a second evaluator, a tree reference language, platform-specific code to portable libraries, or a hidden fallback. Any missing prerequisite requires an explicit dependent bead, not a local substitute.

---

