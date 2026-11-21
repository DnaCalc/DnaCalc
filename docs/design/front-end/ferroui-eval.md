# FerroUI for DNA Calc

## Question and provisional verdict

Should DNA Calc use FerroUI for the native desktop grid, and can the same FerroUI grid compete with the Leptos web app? Keep Leptos and WebView2 as the baseline for the Sunday review. The Linux prototype exercises the calculation and interaction paths. Windows measurements are needed to establish a GPU or browser advantage.

The native candidate needs to match WebView2 at 1080p and improve sustained scrolling and large-range interaction at 4K without slower typing, recalculation, startup or idle behavior. The browser candidate needs comparable interaction and bundle and memory costs that its smaller feature set can justify. Cell accessibility, Japanese IME and per-monitor DPI need working results for a replacement. The Windows evidence belongs to **dc-etcxb.3**.

## Method

- A custom FerroUI control draws visible rows, columns and frozen bands. CalcHost loads the workbook, applies edits and publishes recalculated values for repaint. The formula bar and in-cell editor use FerroUI text boxes. The grid uses a bounded cache of shaped text layouts and the same CalcFunc number formatter as the Leptos worker, with CalcDoc's format codes and workbook date system.
- The prototype has a frozen first row and column, a name box, navigation through XFD1048576, selection, worksheet zoom, and W1's solid fills, bottom borders and merged title. It uses fixed row heights and column widths and shows the saved active sheet when available. The current DNA Calc app covers additional styling, worksheet geometry and workbook features and has a semantic cell layer.
- Both stacks use [scenarios.json](../../../tools/ferroui-eval/scenarios.json). The native runner uses FerroUI platform input for wheel, navigation, typing and commit. It invokes worksheet jump and zoom commands and the window resize API. The CDP driver sends keyboard and wheel events to the running apps. Its S9 viewport override exercises page reflow. The handoff includes a separate Windows OS resize check.
- Linux figures are one release trial per case at a 1920×1080 client size on dna-firehorse (AMD EPYC, Linux 7.0.0-43-generic), with Xvfb and Skia software raster. Other builds shared the host. The CDP drivers passed syntax checks. There were no browser runs on Linux. The Windows run uses fresh profiles, two resolutions and three trials per stack.
- Native timing uses the composition batch completion associated with a grid generation or text-box change. CDP samples requestAnimationFrame during activity and uses observable state and frame callbacks for input latency. These are presentation proxies with different clocks. Native startup starts at process entry. Fresh processes reuse the host filesystem cache. Peak and settled working set use process samples. Edge reports fresh navigation and workbook upload separately. WebView2 startup includes polling for the debugging endpoint.
- [The W1 generator](../../../tools/ferroui-eval/make-w1.py) produced 500,108 bytes with SHA-256 **96a523f8967359454bfb96210c800412636c2d0e0cb91be9e9ede494143d8804**. CalcHost loaded 100,000 inputs and 10,000 formulas with no repairs, unloaded cells or VBA. The formulas occupy B2:U501 and all read A1. S8 verifies all 10,000 results as calculated values after the edit.
- W2 is **EUSES/files/database/database/processed/Database_excel95.xls**, 41,660,928 bytes, SHA-256 **bef98c3eb205153c0b61c77698a9f1df898f7dcda267d57622f9e0b52b44c594**. It was the largest file in the extracted local EUSES population. Static checks found no VBA storage, XLM sheet or encryption marker. CalcDoc loaded 3,292,295 cells across seven sheets, with zero formulas, repairs or unloaded cells. No corpus file ran in Office or supplied executable code. Collections absent from the host were outside the ranking.

## Scenarios

All 11 Linux trials passed their functional checks. Each Linux cell gives wall ms, render-completion interval p50/p95/p99 ms, interval counts over 16.7/33 ms, input-to-paint p50/p95/p99 ms where applicable, and peak/settled working set MiB. S1 has one first-paint observation and S8 one edit observation. S4 and S9 include intentional pauses between actions. S10 had no scenario redraws and used **0.58% of one core**, including the sampler.

| Case | FerroUI Linux raster smoke | Leptos Linux | FerroUI Windows | WebView2 Windows | FerroUI Edge | Leptos Edge |
|---|---|---|---|---|---|---|
| S1 empty cold | 169.2 · — · 0/0 · — · 77.8/77.8 | — | | | | |
| S1 W2 cold | 41,756.3 · — · 0/0 · — · 580.5/448.3 | — | | | | |
| S2 scroll 2,000 rows down and back | 18,710.4 · 50.4/138.6/345.0 · 296/237 · — · 228.2/228.2 | — | | | | |
| S3 Page Down ×200 | 10,761.7 · 26.5/267.8/402.4 · 143/54 · — · 227.4/227.4 | — | | | | |
| S4 far jump and return | 512.8 · 136.6/136.6/136.6 · 1/1 · — · 163.3/163.3 | — | | | | |
| S5 scroll 200 columns | 2,831.7 · 38.5/197.0/311.5 · 46/26 · — · 166.3/166.3 | — | | | | |
| S6 Shift+arrows, 500×50 | 12,725.8 · 29.3/85.1/124.8 · 331/114 · — · 193.7/193.7 | — | | | | |
| S7 200 keystrokes | 6,977.0 · 32.0/77.0/132.8 · 160/89 · 42.5/140.9/332.4 · 163.9/163.9 | — | | | | |
| S8 edit with 10,000 dependents | 120.3 · — · 0/0 · 120.3/120.3/120.3 · 173.0/173.0 | — | | | | |
| S9 resize and zoom 50%–200% | 2,274.6 · 146.3/476.5/476.5 · 13/11 · — · 187.0/187.0 | — | | | | |
| S10 idle 60 s | 60,001.4 · — · 0/0 · — · 154.7/154.7 | — | | | | |

An earlier S2 run shaped labels anew for each draw. It took 87.5 s, with a 247.1 ms median interval and 153.7 MiB peak working set. The measured implementation reuses shaped layouts. That first prototype also lacked the explicit formatting locale. The table uses the en-US rerun on the same shared machine. The trials do not isolate the cost of shaping or formatting.

The stock TreeDataGrid probe used S2's row deltas through its ScrollViewer. The W1 flat source had 2,501 rows and 40 worksheet columns. The control realized 43 rows at all three checkpoints, reaching row 2,001 and returning to row 1 in 13.9 s. It is viable for a finite table. Sparse worksheet addressing, frozen quadrants and direct formatted-cell editing remain custom-grid work. [TreeDataGrid](https://github.com/wieslawsoltes/FerroUI/blob/v0.5.0/docs/porting/treedatagrid.md)

## Builds and sizes

| Output | Raw bytes | gzip bytes | Brotli bytes |
|---|---:|---:|---:|
| FerroUI Linux x64 release executable | 133,268,584 | — | — |
| Same executable, symbols stripped from a copy | 82,511,816 | — | — |
| FerroUI Windows x64 release executable | 82,988,544 | — | — |
| DNA Calc WebView2 package executable | 83,227,136 | — | — |
| FerroUI Edge bundle | 30,307,683 | 11,530,074 | 7,697,042 |
| DNA Calc Leptos bundle, app and worker | 38,348,436 | 11,395,383 | 7,368,345 |

Bundle totals sum each shipped file, with gzip level 9 and Brotli quality 11 applied per file. Totals include lazy assets and exclude source maps. The FerroUI browser prototype runs calculation and rendering in one main-thread module. The Leptos bundle includes a separate calculation worker and ships the current app's wider feature set. The FerroUI site is 21.0% smaller raw, 1.2% larger with gzip and 4.5% larger with Brotli.

- FerroUI and FerroUI View are pinned to **0.5.0** at **d503df2befa756e681fcdc6250e9552dc7388bea**. Skia is **0.153.3**. The Linux release used Rust **1.98.0-nightly** (df6ee909e), eight Cargo jobs and system X11, FreeType and Fontconfig. The final release rebuild with changed CalcDoc sources took 4m 52s. The native target occupied 2.51 GB. Linux linking used two unversioned symlinks to installed libraries under the standalone prototype. [FerroUI 0.5.0](https://github.com/wieslawsoltes/FerroUI/releases/tag/v0.5.0)
- The x64 MSVC cross-build linked a PE32+ executable. It imports the Visual C++ v14 runtime. The compiled runtime prerequisite and actual Windows launch are part of the handoff. The final cached cross-build took 2m 23s after the initial 18m 50s dependency pass. Its target occupied 2.58 GB. The DNA Calc packaged baseline cross-built with its pinned Rust 1.94.1 and static CRT. [Microsoft Visual C++ runtime](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170)
- Browser setup installed Emscripten **6.0.10**, Rust **1.99.0** and wasm-bindgen **0.2.129** in 2m 35s. The SDK and tools occupied 1.72 GB. The optimized site built in 28m 46s, with a 1.86 GB build target. All three shipped wasm modules across both sites passed Node WebAssembly validation. FerroUI uses Emscripten's C++ linker and the WebGL or Canvas bridge. The Leptos release bundle uses DNA Calc's web profile on wasm32-unknown-unknown. The independent Leptos web build took 31m 24s on the shared host. The table uses the staged rebuild, whose page bytes are identical to those embedded in the WebView2 package. [FerroUI browser build](https://github.com/wieslawsoltes/FerroUI/blob/v0.5.0/docs/porting/browser-platform.md) · [DNA Calc packaging](../../../apps/PACKAGING.md)
- The prototype archive contains each target's Cargo package and licence inventory with source URLs and hashes, the upstream browser npm inventory and native notices. Cargo resolved 213 Linux packages, 187 Windows packages and 159 browser packages including local crates. The browser tooling lock has 296 entries. FerroUI is MIT. The inventory also covers Skia's BSD-style licence, HarfBuzz, ANGLE, controls' Ms-PL notice and the Inter OFL-1.1 notice. [Upstream notices](https://github.com/wieslawsoltes/FerroUI/blob/v0.5.0/NOTICE.md)
- Build downloads and approval details are in the archive and Windows handoff. Cargo fetched the pinned FerroUI source for the browser scripts, the crate archives and three Skia binary archives. The upstream setup fetched emsdk, its SDK and Node, Rust components and the wasm-bindgen CLI crate graph. The browser build script fetched the locked npm archives. Individual crate and npm archive URLs are in the inventory. Native Skia came from the pinned Linux and MSVC archives, and the browser used the matching Emscripten archive. The cross-build used the approved local MSVC SDK cache. Windows uses installed Edge and WebView2. [Linux Skia](https://github.com/rust-skia/skia-binaries/releases/download/0.153.3/skia-binaries-b7f043e0b1e2a850e702-x86_64-unknown-linux-gnu-ganesh-gl-jpegd-jpege-pdf-vulkan.tar.gz) · [MSVC Skia](https://github.com/rust-skia/skia-binaries/releases/download/0.153.3/skia-binaries-b7f043e0b1e2a850e702-x86_64-pc-windows-msvc-ganesh-gl-jpegd-jpege-pdf.tar.gz) · [Emscripten Skia](https://github.com/rust-skia/skia-binaries/releases/download/0.153.3/skia-binaries-b7f043e0b1e2a850e702-wasm32-unknown-emscripten-ganesh-gl-jpegd-jpege-pdf.tar.gz)

## Fit

- **Skins.** Styles, selectors, resources, theme variants and XAML templates can implement installed Strand tokens and controls. Workbook CustomXML can carry DNA Calc's validated declarative skin configuration. XAML compilation belongs with installed components. DNA Calc still owns the workbook, selection, undo and calculation model. [Skin discussion](SKIN_DISCUSSION.md) · [FerroUI themes and XAML](https://github.com/wieslawsoltes/FerroUI/blob/v0.5.0/docs/xaml-themes.md)
- **Text and input.** FerroUI shapes with HarfBuzz and draws with Skia. The Linux raster capture showed W1's dates, percentages, custom kg format, error labels, fills and border lines. The installed Linux fonts rendered the Japanese text, with long strings clipped at fixed cell widths. The prototype uses standard text boxes for editing. In the browser, fonts are registered from bytes. The prototype bundles Inter and CJK fallback needs verification. Upstream documents Windows IME and UI Automation with platform smoke coverage. ClearType depends on the surface and display settings. The composition path and high scaling can use grayscale. The custom sheet needs virtual automation peers for individual cells in Narrator and the browser's on-demand ARIA mirror. [Text rendering](https://github.com/wieslawsoltes/FerroUI/blob/v0.5.0/docs/porting/text-rendering.md) · [Manual verification](https://github.com/wieslawsoltes/FerroUI/blob/v0.5.0/docs/porting/manual-verification.md) · [Browser accessibility](https://github.com/wieslawsoltes/FerroUI/blob/v0.5.0/docs/porting/browser-accessibility.md)
- **Application work.** Formula parsing and completion data can reuse DNA Calc libraries. The current formula editor's highlighting, completion popup, composition and selection need a new presentation. FerroUI has chart controls. Workbook bindings, imported chart fidelity and edit operations remain DNA Calc work. The canvas/Flow skin needs link rendering, pan, zoom and interaction. The Skia prototype leaves the planned WebGPU grid unevaluated.
- **Project maturity.** The November 2026 release includes UI composition, XAML tooling and backend work and lists Wiesław Šoltés as its release contributor. Upstream keeps the public API within 0.5 and explicitly allows changes in 0.6. [Release and activity](https://github.com/wieslawsoltes/FerroUI/releases/tag/v0.5.0)

## Handoff

The harness is in [tools/ferroui-eval](../../../tools/ferroui-eval/). The standalone prototype, lockfile and inventories are in **~/codex-runs/fu1/ferroui-eval-src.tar.gz** on dna-firehorse. The Windows lane uses **~/codex-runs/fu1/RUN-ON-WINDOWS.txt** and the hash-checked stage with both executables, both sites and both workbooks. Per-case Linux JSON and build logs are in that run directory. Windows hardware, CDP behavior, accessibility, IME, text and DPI remain open in **dc-etcxb.3**.
