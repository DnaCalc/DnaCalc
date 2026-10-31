> **Owner ruling 2026-10-31 (supersedes the licence conclusion below):** the rider keeps OpenAI/Anthropic themselves from taking the code for their own products; it does not stop us, using coding agents on our own behalf, from depending on, reading, fixing or improving these projects. asupersync is acceptable as a dependency, with a pointer to the rider in third-party notices; optionally ring-fenced behind a clean boundary with a simple non-concurrent path we maintain. The technical findings below stand.

# Research brief: libraries for DNA Calc's next-generation engine

Date: 2026-10-31. Sources fetched from the web on that date. Tags: **[V]** means verified on the cited page today.
**[K]** means prior knowledge that was not re-checked today. **[I]** means my inference. Star counts are point-in-time.

---

## 1. asupersync (Jeffrey Emanuel / Dicklesworthstone)

Repo: https://github.com/Dicklesworthstone/asupersync. Crate: https://docs.rs/crate/asupersync/latest.
README: https://raw.githubusercontent.com/Dicklesworthstone/asupersync/main/README.md.
Licence: https://raw.githubusercontent.com/Dicklesworthstone/asupersync/main/LICENSE.

### What it is (verified)
- An async runtime for Rust, presented as an alternative to tokio, with the tagline "Async runtime for Rust where correctness is structural". [V]
- **Runtime model.** The native runtime uses a multi-thread work-stealing scheduler (owner LIFO, thief FIFO) plus a blocking pool, and a current-thread flavour is available. [V]
  - Work is dispatched through three lanes in a fixed order: a Cancel lane (priority 200-255), then a Timed lane ordered earliest-deadline-first, then a Ready lane. [V]
  - Cancel preemption is bounded by `cancel_streak_limit=16`. [V]
  - `!Send` local tasks are pinned to their worker. [V]
- **Structured concurrency.** Every task belongs to a region, and "region close waits for all children". A region also cannot close until its obligations are resolved. `race!` cancels and drains the losers before it returns. [V]
- **Cancellation.** The protocol is request, then drain, then finalize. Cancellation is cooperative: a task stops only at `cx.checkpoint()` or at a cancel-aware await. [V]
  - A task that never reaches a checkpoint "is never forcibly stopped", and it holds up its region's close. [V]
  - `cx.masked(f)` defers cancellation, and finalizers run masked and budgeted. [V]
  - Budgets carry `deadline`, `poll_quota`, `cost_quota` and `priority` (0-255), but they are *advisory* on the production runtime. The deadline monitor only warns. [V]
  - `Outcome` has four values: Ok, Err, Cancelled and Panicked. [V]
  - Channels and oneshots use two-phase reserve/commit sends so that cancellation is safe. [V]
- **Determinism (lab runtime).** The lab runtime gives virtual time, seeded deterministic scheduling ("same seed, same execution") and trace capture and replay. [V]
  - A DPOR-style explorer exists but makes no completeness guarantee. [V]
  - 11 of the 24 oracles are fed from runtime state. [V]
  - The Lean proofs cover six invariants of an abstract model, not the Rust code. [V]
- **Capability model.** Spawning, timers and effects go through an explicit `&Cx`. [V]
  - Most I/O entry points take no `Cx`. They check the caller's context at runtime and refuse with error ASUP-E009 when the capability is missing. [V]
  - The README lists capability gaps around I/O and `spawn_blocking` as tracked work. [V]
- **Wasm/browser.** It compiles to `wasm32-unknown-unknown`, and there is a JS/TS "Browser Edition" built with wasm-bindgen. [V]
  - The browser runtime "is single-threaded" and runs on the main thread and in dedicated workers. [V]
  - Service and shared workers get a broker/coordinator role only. [V]
  - On the browser local executor, cancellation "drops the future instead of draining it". [V]
  - The README says "no wasm-side scheduler loop yet" and "no browser-engine test runs in CI". [V]
  - The JS packages are a release candidate that has not been published to npm. The Rust browser lane is marked "preview". [V]
- **Other surface.** The crate also provides:
  - Spork, an OTP-style actor and supervisor layer. [V]
  - MPSC, broadcast, watch and session channels. [V]
  - Fair cancel-safe Mutex and RwLock. [V]
  - Combinators: quorum, hedge, bulkhead, retry, circuit-breaker. [V]
  - A "remote runtime" with RaptorQ snapshots. The README does not claim multi-host or WAN operation. [V]
- **Maturity.** The README says "Experimental, pre-1.0" and "no known independent production user". [V]
  - Latest version on docs.rs is 0.5.0 (2026-09-12). There were about 30 releases since 0.1.0 (2026-02-04), and several early ones were yanked. [V]
  - It uses edition 2024. Default features build on stable Rust 1.95, but `?` on `Outcome` needs nightly, and contributors use a pinned nightly. [V]
  - There is no semver or API-stability promise. [V]
- **Size and activity.** The repo shows 25,055 commits on main, 288 stars and 32 forks. The source crate is 57 MB. [V]
  - Documentation coverage is 100% across 47k items. [V]
  - About 100 dependency entries. [V]
  - GitHub shows it was updated on 2026-10-31. [V]
- **Performance (author's own figures).** Spawn plus join is 12-23x slower than tokio, and a task "costs several times what a tokio task costs". [V]
- **Dependents.** These are almost all the author's own crates: sqlmodel-*, frankensearch-*, franken-snowflake-*, ftui-runtime, frankenlab and others. A handful come from third parties: oraclemcp-*, pureflow-*, koompi-kesa. See https://crates.io/api/v1/crates/asupersync/reverse_dependencies. [V]
- **Governance.** "I do not accept outside contributions for any of my projects". Issues and illustrative PRs are welcome, but the author won't merge them. [V]
- **Licence.** MIT plus an "OpenAI/Anthropic Rider" that "controls" in any conflict. [V]
  - It grants "no rights" to OpenAI, Anthropic, their affiliates, or anyone acting "on behalf of, for the benefit of, or under the direction of" them. [V]
  - It forbids anyone to "provide, disclose, distribute ... or otherwise permit access" to those parties. [V]
  - Its definition of "use" includes "analyzing, indexing" and incorporation into "any ... evaluation harness, or pipeline for ... automated systems". [V]
  - Distributions must carry the rider unmodified, and a breach terminates the licence automatically. [V]

### Fit for DNA Calc's engine (inference unless tagged)
- **Recalc scheduling.** [I] The fit is strong *in concept*.
  - A region per recalc wave, with child tasks per dependency stratum, matches the shape of a recalc.
  - "Region close implies quiescence" gives a clean "this recalc is done or fully abandoned" point.
  - The EDF lane and per-task `priority` could express "visible grid region first".
  - However, priority is a budget field, not a re-prioritisable queue. Raising priority when the viewport moves means cancelling and respawning, not reordering.
- **Cancelling stale recalcs.** [I] Request/drain/finalize semantics are a good fit for "view moved, abandon off-screen work". There are two catches:
  - Formula evaluation is CPU-bound synchronous code. Cancellation reaches it only if the evaluator calls `cx.checkpoint()` every N cells. That is a design choice DNA Calc must make whatever runtime it uses.
  - Two-phase commit is useful for publishing computed values to the grid only when a wave commits.
- **Distributing work across workers or threads.** [I] It is a poor fit for fine-grained cell work.
  - A per-task overhead of several times tokio's rules out one task per cell. Tasks would have to be coarse: per chunk or per stratum.
  - CPU-parallel evaluation of independent cells is better done with rayon or a custom chunk scheduler *inside* a task, which asupersync cannot see or cancel.
  - The remote runtime makes no multi-host claim, so it does not deliver the distributed or billion-node goal.
- **Deterministic testing of concurrent recalc.** [I] This is the strongest argument for it. Seeded replayable schedules and virtual time would let DNA Calc pin "recalc A cancelled mid-wave by edit B" interleavings as regression cases.
  - Comparable alternatives that need no runtime switch: shuttle (awslabs, Apache-2.0, randomised/PCT schedule testing) and loom for model-checking small kernels. [K]
- **Browser (wasm32-unknown-unknown in a web worker).** [V+I] It is weak today.
  - The runtime is single-threaded and preview-grade. There is no wasm-side scheduler loop, no browser CI, and cancel-by-drop rather than drain.
  - A worker-pool engine in the browser would still need its own multi-worker layer: SharedArrayBuffer with COOP/COEP headers, or message-passing workers.
  - The native and browser lanes have different semantics, which undermines "same engine everywhere".
- **Native.** [I] It works, but needs nightly for ergonomic `Outcome` handling. The crate is large (about 100 deps, 57 MB source), so builds get heavier, which is a cost given the known shared-target ICE history.

### Risks
1. **Licence (decisive).** [I] DNA Calc is built and reviewed by Claude agents, and Anthropic is a named Restricted Party.
   - Feeding asupersync source into Anthropic-hosted models plausibly counts as "disclose ... or otherwise permit access" and as "analyzing" in an "automated system".
   - The published product would have to carry the rider, and DNA Calc could not lawfully be provided to OpenAI or Anthropic or their affiliates.
   - This is a legal question for counsel. On the plain text, it conflicts with the project's AI-agent workflow and with an unrestricted publication.
   - The same rider is on frankensqlite, frankentui, frankenscipy, frankengraphdb and franken_engine (section 2).
   - Disclosure: this brief was produced by Claude. It read only the public README and LICENSE pages, not the source.
2. **Maturity.** Pre-1.0, no stability promise, a fast release cadence, yanked early versions, a nightly dependency, and some oracles and proofs unfinished. [V]
3. **Wasm.** The browser lane is preview, single-threaded and untested in CI. [V]
4. **Single maintainer, no outside merges.** The bus factor is 1, and DNA Calc fixes would have to be forked or upstreamed through issues. [V+I]
5. **Performance.** Per-task overhead runs against "extreme optimisation" unless task granularity is coarse. [V+I]

**Recommendation (inference).**
- Do not adopt it as the base. The licence alone settles that.
- Do borrow the design ideas, which are not licensed code: region-owned recalc waves, request/drain/finalize cancellation with explicit checkpoints, reserve/commit publication of results, cancel-lane priority, and seeded lab replay.
- Implement those ideas over a small in-house scheduler that also targets wasm workers.

## 2. Other work by the same author (verified to exist; agent-workflow tools skipped)

Repo list: https://github.com/Dicklesworthstone?tab=repositories&language=rust (82 Rust repos).
Every one checked carries the same MIT plus OpenAI/Anthropic rider. That rules out using any of them as a dependency of DNA Calc for the reason in risk 1.

- **frankensqlite** (`fsqlite`, https://github.com/Dicklesworthstone/frankensqlite). [V]
  - A ground-up Rust reimplementation of SQLite with page-level MVCC and concurrent writers, compatible with SQLite 3.x files.
  - The README calls it "real, but still hybrid". It needs nightly, depends on asupersync, and its wasm build has memory-only persistence (OPFS and IndexedDB raise errors).
  - Possible use as a workbook or session store. rusqlite or plain SQLite is safer.
- **frankentui** (https://github.com/Dicklesworthstone/frankentui). [V]
  - A terminal UI kernel of about 20 crates, with a diff renderer, Fenwick-tree variable-height virtualised lists, and a wasm/WebGPU web backend.
  - Status is WIP and it needs nightly.
  - Only relevant as a reference for virtualisation, or for a TUI front end.
- **frankengraphdb** (https://github.com/Dicklesworthstone/frankengraphdb). [V]
  - A property-graph database with an MVCC commit stream, `AS OF` time travel and `SUBSCRIBE` changefeeds. A DBSP-style engine is planned.
  - Much of the README describes a target state. It is built on asupersync and has no wasm.
  - Only a design reference for versioned dependency graphs.
- **frankenscipy** (https://github.com/Dicklesworthstone/frankenscipy). [V]
  - A clean-room port of SciPy: special functions (gamma, beta, erf/erfinv, Bessel, hypergeometric) and 95+ distributions.
  - It is differentially tested against SciPy 1.17.1 under named tolerances, not bit-exact.
  - Relevant only as a *reference* for statistical functions. Excel's bit-exact oracle remains Excel itself.
- **franken_engine** (https://github.com/Dicklesworthstone/franken_engine). [V]
  - A native Rust JS/TS engine (no V8 or QuickJS) with a typed-capability sandbox for untrusted extensions.
  - Self-described "research-grade", v0.2.0.
  - A conceptual match for agent-written skins or extensions, but not usable.
- **frankensearch** (https://github.com/Dicklesworthstone/frankensearch) is hybrid BM25 plus embedding local search. [V: exists, description only]
- **franken_numpy** and **frankenpandas** are clean-room NumPy and pandas ports. [V: exist, descriptions only] They are not needed, because Arrow covers columnar data.
- Tools the project already uses: `beads_rust` (br) and `beads_viewer_rust` (bv). [V]
  - These are tooling, not product dependencies, so the rider matters less there. [I]
  - Counsel should still note that Claude agents operate them. [I]

## 3. Other projects worth considering

**salsa** (https://github.com/salsa-rs/salsa, https://docs.rs/crate/salsa/latest).
- Facts:
  - "A generic framework for on-demand, incrementalized computation", inspired by Adapton and rustc's query system. [V]
  - Licensed Apache-2.0 or MIT. Version 0.28.5, released 2026-09-24. About 3.0k stars. [V]
  - Cancellation unwinds in-flight queries with `Cancelled::PendingWrite` when a write moves the database to revision R+1. [V: https://docs.rs/salsa/latest/salsa/enum.Cancelled.html]
  - Used by rust-analyzer and Astral's ty. [K]
  - Supports durability levels and early cutoff (backdating). [K]
- Fit [I]:
  - It is an excellent model for demand-driven evaluation: pull only what the viewport needs, with free memoisation and invalidation.
  - Its granularity is per query, with per-query overhead and a hash-map memo. A billion cells as salsa queries is unrealistic.
  - It suits sheet-level or block-level derived data (names, parsed formulas, range dependency summaries) better than per-cell values.
  - Its unwind-based cancellation is crude compared with a viewport-priority scheme.

**Adapton** (https://github.com/Adapton/adapton.rust).
- MPL-2.0. The repository was archived on 2026-09-08. [V]
- Useful only as theory (demanded computation graphs). Do not depend on it.

**differential dataflow / timely** (https://github.com/TimelyDataflow/differential-dataflow).
- MIT, about 3.0k stars, research-grade but long-lived. [V]
- Fit [I]:
  - Its strengths are distributed, data-parallel incremental computation over collections, including iteration.
  - A spreadsheet's irregular per-cell formula graph, with volatile functions and Excel's evaluation-order quirks, does not map naturally onto relational operators.
  - It is worth studying for the distributed-recalc goal: progress tracking and frontiers.
  - It is not a recalc engine. There is no wasm story. [I]

**DBSP / Feldera** (https://github.com/feldera/feldera, https://docs.rs/crate/dbsp/latest).
- Incremental computation whose updates are "proportional to the size of the change". [V]
- Repo licence MIT. `dbsp` is at 0.373.0, released 2026-10-31 (very fast churn). About 2.1k stars. [V]
- No wasm. [V: not mentioned]
- Fit [I]: the same relational mismatch as differential dataflow. It is a good fit for "spreadsheet over a streaming table" features (Power Query or data model style), not for core cell recalc.

**Apache Arrow (arrow-rs)** (https://docs.rs/crate/arrow/latest).
- Version 60.1.0, released 2026-10-28. Builds for `wasm32-unknown-unknown` with default features off. [V]
- Apache-2.0. [K]
- Fit [I]:
  - Strong for chunked columnar storage of *values*: typed columns, null bitmaps, zero-copy slices, and an IPC/FFI boundary to Python or JS.
  - Weak for mixed-type, sparse, mutable cells. You need a dense-chunk-plus-overlay design, which formualizer already uses (below).
  - The dependency weight on wasm needs measuring.

**IronCalc** (https://github.com/ironcalc/IronCalc).
- A "work-in-progress spreadsheet engine" with an xlsx reader and writer, wasm and Node bindings, and a browser preview. [V]
- MIT or Apache-2.0, about 4.2k stars. [V]
- The recalc model is not documented on the README. Its API shows an explicit `model.evaluate()`. [V]
- Fit [I]: a useful comparator and a source of xlsx and locale handling ideas. Its whole-model evaluate approach doesn't target demand-driven or billion-cell recalc.

**formualizer** (https://github.com/psu3d0/formualizer, https://docs.rs/crate/formualizer-eval/latest).
- An "Arrow-backed Excel formula engine with dependency graph and incremental recalculation". [V]
  - Uses a CSR edge format, cycle detection, and optional rayon parallelism.
  - Claims 320+ functions.
  - Has Python (PyO3) and wasm/npm bindings.
  - Version 0.13.0, released 2026-10-25. About 205 stars. MIT or Apache-2.0.
  - Calls itself "the fastest open-source spreadsheet engine" while saying formal benchmarks are in progress.
- Fit [I]:
  - It is the closest existing match to DNA Calc's storage and graph direction: Arrow chunks with spill overlays, and a CSR graph.
  - Worth a benchmark comparison against DNA Calc's own measured Excel corpus.
  - Its parallelism uses rayon with no cancellation or viewport priority.

**windows-rs** (https://github.com/microsoft/windows-rs).
- MIT or Apache-2.0, about 12.9k stars. [V]
- `windows-core` provides `#[interface]` and `#[implement]` for authoring COM. `windows-bindgen` and `windows-metadata` generate code from ECMA-335 (winmd) metadata. [V]
- The README does not mention typelib or IDispatch. [V]
- Fit [I]:
  - The natural base for in-process COM interop on Windows, for example exposing an Excel-like object model to a VBA engine, or hosting existing COM add-ins.
  - Typelibs are not winmd. Early binding from `.tlb` needs either a converter (tlb to RDL/winmd to bindgen) or a custom generator.

**msft-typelib** (https://docs.rs/crate/msft-typelib/latest).
- An "allocation-free parser for MSFT-format type library (.tlb) files". Version 0.2.0, released 2026-10-10. Apache-2.0. A small, new third-party crate (ATRAPSLLC). [V]
- Fit [I]:
  - Could feed a cross-platform early-binding layer: the VBA compiler resolves vtable slots and DISPIDs from the .tlb without Windows.
  - Its youth means it should be vendored and tested against the Excel and Office typelibs.

**intercom** (docs.rs/crate/intercom) generates typelibs from Rust COM components, at version 0.4. [V: exists] I have not checked whether it is still maintained.

**Wasm component model / WIT** (https://component-model.bytecodealliance.org/, https://github.com/bytecodealliance/jco).
- WASI 0.3.x is current and adds async `future` and `stream`. [V]
- jco transpiles components into ES modules for browsers and Node. It is Apache-2.0 with the LLVM exception. [V]
- wasmtime (Apache-2.0, about 18.8k stars) runs components natively. [V]
- wit-bindgen and cargo-component are the Rust tooling. [K]
- Fit [I]:
  - WIT is a good fit for the plugin, add-in and skin boundary: typed, language-neutral, sandboxed, and the same interface natively (wasmtime) and in the browser (jco).
  - Costs: copying across the boundary per call, so plugins should get range or batch calls, not per-cell calls. Browser support goes through transpilation rather than native component support.

**Cranelift** (https://docs.rs/crate/cranelift-codegen/latest, inside wasmtime).
- Version 0.137.1, released 2026-10-29. [V]
- Licence Apache-2.0 WITH LLVM-exception. Backends are x86-64, aarch64, riscv64 and s390x. [K]
- Fit for VBA and formula JIT [I]:
  - Native JIT is credible, and cranelift-jit and cranelift-module exist. [K]
  - It **cannot JIT inside the browser**, because wasm32 cannot create executable memory.
  - In the browser, the equivalent is to emit a wasm module at runtime (for example with `wasm-encoder`) and instantiate it through JS.
  - A shared mid-level IR with two backends (Cranelift native, wasm emission in the browser) is the realistic design.
  - The weekly 0.x breaking releases mean the version must be pinned.

**Browser threading helpers.**
- wasm-bindgen-rayon (rayon over web workers with SharedArrayBuffer, which requires COOP/COEP headers). [K]
- calamine (MIT, xlsx/xlsb reading). [K]
- Both are worth listing for the "same engine in a worker pool" and import paths.
