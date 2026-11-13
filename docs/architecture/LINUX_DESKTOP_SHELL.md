# DNA Calc desktop on Linux: shell options (dc-icq4.3)

Design note, 2026-11-13. Option B is implemented by dc-icq4.4; options A and C remain proposals.
WebKitGTK and WebGPU on WebKitGTK have not been measured on this host (see below).

## What the Windows shell is today

- **Windows-only pieces.** `platform/calc-platform-desktop/src/lib.rs` compiles the shell
  (`shell`, `webview`, `winhttp`, `dialogs`, `background`) under `#[cfg(windows)]`; its
  `webview2-com` and `windows` dependencies sit under `[target.'cfg(windows)'.dependencies]` in its
  `Cargo.toml`. Only `assets`, `macro_security` and `origin` build on Linux.
- **The shell is skin deep.** `apps/dna-calc-desktop/src/app.rs` (header comment): the page, the
  protocol and `dna_calc_worker::server::Server` are the ones the browser runs; the shell carries
  JSON texts and tile bytes between the page and a `Server` on the STA thread.
- **The page side is one file.** `apps/dna-calc/web/desktop-bridge.js` replaces the page's `Worker`
  with `chrome.webview.postMessage`; framing is `text U+0001 base64(bytes)`, channels per Worker.
- **The page is served in memory.** `WebView::serve_assets` (`webview.rs`) answers
  `https://dnacalc.app/*` from an asset block embedded in the executable
  (`apps/dna-calc-desktop/src/embedded.rs`), or `map_folder` maps a folder.
- **Linux today:** `apps/dna-calc-desktop/src/main.rs` compiles `app`, `embedded`, `engine` and
  `fetch` only under `cfg(windows)`; on Linux only `file_import` and `origin` build.
  `dna-calc-worker` is `portable` and ships a headless `InProcessTransport` (`transport.rs`), so the
  server itself runs natively on Linux (the runner `apps/dnacalc` uses `calc-host` directly).
- **WebGPU.** The only WebGPU code in the repository is the measurement page
  `apps/dna-calc-desktop/probe/webgpu.html`, run by `dna-calc-desktop --probe`. The page's grid is
  DOM or Canvas2D (`docs/architecture/RENDERER_COMPARISON.md`). "WebGPU-friendly grid from the
  start" is a rule (`review-2026-11/PRO10_PROGRAMME.md` line 53), not yet code. The server already
  delivers tile bytes, which is what a GPU grid wants.

## Add-ins, COM, VBA on Linux (facts)

- **XLL: none.** An XLL is a Windows DLL that imports `XLCALL32`; `calc-xll-host` is `native-only`
  and described as "Windows XLL loading" (`crates/calc-xll-host/Cargo.toml`), the desktop executables
  export `MdCallBack12` (`apps/dna-calc-desktop/src/main.rs`). A Linux process cannot load it. A native `.so` add-in ABI would be a new
  DNA Calc extension, not Excel, so it is out of scope (the engine does no more than Excel).
- **COM and RTD: none.** `calc-com-host` is `native-only` and answers `WM_GETOBJECT` on the shell's
  window chain; `calc-addin-host` modules are `cfg(windows)` (`crates/calc-addin-host/src/lib.rs`).
- **VBA works.** `calc-vba` and `calc-host-vba` are portable or build on Linux; the runner's `run`
  and `--macro-level safe|unguarded` execute a file's project (tests in `apps/dnacalc/tests/cli.rs`).
  `safe` refuses operating-system services; what `unguarded` should mean for `Shell`, `Environ` and
  file paths on Linux is open (see Open questions).
- So on Linux every shell option has the same add-in answer: VBA modules and VBA `.xlam` add-ins
  yes through the server, XLL and COM add-ins a typed refusal naming the platform.

## Options

### A. wry (WebKitGTK) shell

A Linux module in `calc-platform-desktop` (target-gated `wry`/`gtk`, no product dependency, like
the Windows modules), a Linux `app` in `dna-calc-desktop`, and a second carrier in
`desktop-bridge.js` (`window.ipc.postMessage` and an evaluate-script callback instead of
`chrome.webview`; same framing). Assets through wry's custom protocol, replacing `serve_assets`.
- For: closest to Windows; same page, same `Server`, same bridge framing; file dialogs and file
  open natively; no browser needed.
- Against: a dynamically linked desktop executable that needs `libwebkit2gtk-4.1` (Ubuntu 25.10
  offers 2.52.3 by `apt-cache policy`); engine versions differ per distribution, so page behaviour
  varies where WebView2 is one evergreen runtime; Wayland and X11 differences; the static runner
  tarball cannot carry it. WebGPU in WebKitGTK is **not verified**: the probe is the test.
- Not in `Cargo.lock` today (`grep wry Cargo.lock` finds nothing): a new dependency set to vet.

### B. Browser-served local app (`dnacalc serve`)

The runner binary serves the same embedded page on `127.0.0.1:<port>` and bridges a WebSocket to a
native `Server`; the user's own Chrome or Firefox shows it. `apps/dnacalc/src/serve.rs` owns the
listener and native Server channels; `desktop-bridge.js` selects the WebSocket carrier only when
the served page requests it. WebView2 continues to select `chrome.webview`.
- For: stays static (musl) and ships in the tarball; the user's browser brings its own WebGPU,
  fonts and accessibility; no GTK or WebKit dependency; works on any distribution.
- Against: a local server is an attack surface: bind to loopback only, a random token in the URL,
  Origin and Host checks (DNS rebinding), no CORS. Browser download replaces the save dialog;
  native file open goes through the file input or File System Access (Chromium only). The page's
  CSP (`deploy/caddy/headers.caddy`, `connect-src 'self'`) must allow the local socket.
- The hosted web app (`dnacalc.com`, wasm worker) already gives a Linux user the engine in a
  browser. B adds the native worker (threads, speed, local files), not new function.

### C. Native UI, later

A native toolkit (for example wgpu with a text stack) drawing the grid itself from the server's
tile bytes. `dna-strand` and `dna-formula-ui` carry design tokens and the formula UI as portable
code, so the model is shared, but the page's layout, text editing, IME, accessibility and menus
would all be redone: the cost the "skin deep" shell was designed to avoid.
- Revisit when the WebGPU grid exists and measures better than Canvas2D in a browser; the GPU grid
  could then be one renderer for the browser (WebGPU) and native (wgpu). Not before.

## Recommendation

1. **First (done, dc-icq4.1/.2):** the static runner as tarball and `.deb`. It does diff, get, set,
   VBA `run` and save with no GUI.
2. **Next:** option B as `dnacalc serve`, because it ships in the static tarball, needs no new
   system libraries and gives a Linux user the full page with the native worker. Size: the server
   and bridge carrier (`desktop-bridge.js` gets a WebSocket carrier beside `chrome.webview`), a
   token and origin policy, and a smoke run of the existing page suite against it.
3. **Then:** option A, if the WebKitGTK measurement says the page and its smoke suite pass on it
   and users want a window rather than a tab. It reuses B's bridge carrier work for the page side.
4. **Later:** option C, only on the WebGPU grid result.
The Linux answer for add-ins stays the typed refusal above, on every option.

## What a Linux user gets first

`apt install ./dnacalc_<version>_amd64.deb` (or the tarball): open an `.xlsx`/`.xlsm`, recalculate,
compare with Excel's cached values, read and write cells, run VBA at `safe` or `unguarded`, save.
Beside it, the web app in the browser with the wasm worker. No XLL, COM or RTD add-ins.

`dnacalc serve` opens the workbook page over the native worker: visit the URL printed on the
terminal, open files with the browser's file picker, and save to its downloads. Ctrl+C stops the
server. The URL has a fresh 256-bit operating-system random capability in its path on each start;
keep it private. The listener binds only to IPv4 loopback; the port defaults to an available port
(`--port N` overrides it). HTTP checks Host, Origin when present, and cross-site fetch metadata;
WebSocket upgrades require the exact Origin too. No response enables CORS. CSP allows the page's
scripts and its local socket, and the referrer policy withholds the private URL.

The Linux packager builds and embeds the same page block the Windows shell uses. For development:

```sh
cargo xtask app bundle calc --pack target/calc-page.block
DNACALC_CALC_PAGE="$PWD/target/calc-page.block" cargo build -p dnacalc
target/debug/dnacalc serve
```

An everyday build without an embedded page uses `dnacalc serve --assets target/dnacalc-bundle`.
Macro security starts Off, as in the browser worker; this change adds no native file, fetch, XLL
or COM services to the Server.

## Open questions (to measure before building)

- Does the page run, and does `navigator.gpu` exist, in WebKitGTK 2.52? **Not measured on
  dna-firehorse, 2026-11-13:** pkg-config, the dynamic linker cache and Python's library lookup
  found no WebKitGTK 4.1, 4.0 or 6.0 library; WebKitWebDriver is absent too. Neither the page nor
  `apps/dna-calc-desktop/probe/webgpu.html` could be run there without installing the system
  runtime. No system packages were installed. A host with WebKitGTK is still needed (dc-icq4.5).
- What should `Shell`, `Environ`, `CurDir` and path separators mean in `unguarded` on Linux?
- Does File System Access matter, or is download-and-open enough for the browser route?
- aarch64 Linux: the runner build is x86-64 only so far (dc-icq4.2).
