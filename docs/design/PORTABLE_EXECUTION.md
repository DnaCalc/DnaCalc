# Portable execution

Bead `dc-54aki`, lane pt1, 2026-11-28. Spec, architecture and detailed design for **portability**
as the axis orthogonal to Guarded and Unguarded execution: can the code a workbook or add-in
brings run on this host (Windows without Excel, macOS, Linux, a headless server, the web hosted by
us, the web as downloaded wasm, WASI), and what does it need there.

It extends, and does not repeat:

- CalcVBA `docs/design/portability.md` and epic `dc-8svfs` (two dialects, hosts, the VBA library
  matrix, modes, the shim list);
- [GUARDED_EXECUTION.md](GUARDED_EXECUTION.md) and
  [CODE_EXECUTION_POLICY.md](../architecture/CODE_EXECUTION_POLICY.md) (the authority axis:
  origins, reach, capability names, one `decide`, mediated facets, the audit);
- [API_FACES.md](../architecture/API_FACES.md) (one descriptor core, three faces) and the skin
  contract's `Capabilities` and `Refusal::NotAvailableHere { capability, route }`
  (`crates/calc-skin/src/workbook.rs`).

Nothing here is built except the measurement (section 9) and its two scripts. Data files are in
[portable-execution/](portable-execution/). Owner rules hold throughout: the engine does no more
than Excel, Excel's behaviour is matched where Excel defines it, no security setting changes, and
Guarded stays the authority axis.

## Summary

- **Two questions, one vocabulary.** A capability is a facet the host can hand to code. Guarded
  asks *may this code have the facet?* Portability asks *can this host build the facet at all, and
  how?* Each member of every API face declares two sets over the same names: its `reach` (exists
  today, 28 object-model members and one command) and its `needs` (new).
- **Verdicts are computed, never written per cell.** A member says what it needs. A host profile
  says how it provides each need: `native`, `model`, `shim`, `host`, `replace`, `mediated` or
  `reject`. The (member, host) matrix is their product. These are CalcVBA's six words plus
  `mediated`; section 3 maps the owner's, Guarded's and the skin's words onto them.
- **A host is a machine on which some things are not installed.** Excel already defines what
  happens then: a class that is not registered raises 429, a DLL that is absent raises 53 at the
  first call of the Declare, a drive that does not exist raises 68 or 76. A portability refusal
  reaches VBA as that Excel error and reaches the host as a typed `NotAvailableHere`. So "lazy" is
  not a new behaviour, it is Excel's.
- **Paths are resolved, never rewritten.** `c:\Temp\MyFile` stays that string inside the macro.
  The file facet resolves it through a per-session mount table (Plan 9 namespaces, Wine's drive
  map). The same table, with fewer mounts, is what a Guarded file grant is.
- **COM without COM.** VBA needs the Automation object protocol (invoke by DISPID, type
  information, enumerators, events, identity). It does not need activation, apartments,
  marshalling, monikers or a registry. A table of portable classes keyed by CLSID and a table of
  type libraries keyed by GUID replace them; no DCOM.
- **What runs on Linux today (measured, section 9).** Run, Guarded, headless: of 2,449 entry
  points in 976 admitted triage-clean projects, 1,104 succeed; 14 are denied a capability; 53
  stop at a `MsgBox` or `InputBox` the CLI cannot show; 1,241 stop at product gaps that have
  nothing to do with the platform. Scanned: of 1,324 distinct triage-clean projects, 1,099
  (83.0%) hold no procedure that needs anything a Linux desktop lacks; 67 need the path map, 134
  a replacement class or the forms runtime, 24 hold something that cannot run. The
  triage-suspect projects, scanned and never run, are the opposite: 46 of 532 need nothing and
  204 hold a procedure that cannot run off Windows.

## 1. The model

### 1.1 One vocabulary, two questions

CODE_EXECUTION_POLICY 2.2 names ten capabilities and puts each host member's `reach` in the
descriptor (`crates/calc-object-model/descriptor/reach.json`, loaded by `descriptor.rs`). Guarded
hands CalcVBA mediated facets for them (4.2). The same facets are the portability question, seen
from the other side:

| | Question | Answered from | Answer | Who can change it |
|---|---|---|---|---|
| **Guarded** | May this code use the facet? | origin, level, grants | allow, or deny with the VBA error | the person |
| **Portability** | Can this host build the facet, and how? | the host profile | a verdict (section 3) | nobody at run time; the build and the host configuration |

A call goes through both, portability first, because a grant for something that does not exist
means nothing:

```text
member call -> needs ⊆ provided(host)?  no  -> NotAvailableHere { capability, route }  -> Excel's error for "not on this machine"
                      yes
            -> decide(reach, origin)?   deny -> PolicyRefused, audited                  -> Excel's error for "permission denied" (70, 75, 429)
                      allow
            -> the facet does it (native, shim or replacement), audited
```

The four corners, with one example each:

| | Host can | Host cannot |
|---|---|---|
| **Policy allows** | runs: `Open "c:\Temp\out.csv" For Output` on Linux, Unguarded, through the path map | typed refusal: `CreateObject("Outlook.Application")` on Linux, 429 |
| **Policy denies** | typed denial: the same `Open` in Guarded without a file grant, 75 | refusal, reported as *not available*: a denial would suggest a grant could help |

### 1.2 The names

The ten Guarded names stay as they are. Portability needs more names, because a host can lack
things that carry no authority (a message box, a blocking wait), and it needs qualifiers on the
file names, because the facet exists everywhere but its shape does not.

| Name | Covers | Guarded capability | New |
|---|---|---|---|
| `fs.read`, `fs.write` | file statements and members | yes | |
| `path.windows` | drive letters, `\`, UNC, per-drive current directory, case-insensitive names | qualifier of `fs.*` | yes |
| `fs.attributes` | hidden, system, archive bits | qualifier | yes |
| `env`, `settings`, `process`, `ffi`, `com.activate`, `ui.keys`, `network`, `project.write` | as CODE_EXECUTION_POLICY 2.2 | yes | |
| `native.code` | loading machine code of one binary format: `xll` (PE x64), `dll`, `dylib` | part of `ffi` | yes |
| `com.os` | an operating-system COM runtime: registry, class factories, running objects | part of `com.activate` | yes |
| `ui.dialog` | `MsgBox`, `InputBox`, alerts | **should be** (GUARDED_EXECUTION 6: dialogs can lie) | yes |
| `ui.forms` | UserForms and MSForms controls | no | yes |
| `ui.filedialog` | `GetOpenFilename`, `FileDialog`, `Dialogs(...)` | **should be**: the answer is a file grant | yes (skin has `NativeFileDialogs`) |
| `clipboard` | the system clipboard | **should be**: reads data outside the workbook | yes (skin has `ExcelClipboardFormats`) |
| `print` | printers and preview | **should be** | yes (skin has `Printing`) |
| `open.external` | hand a document or URL to another program (`FollowHyperlink`, `ShellExecute "open"`) | today `network` + `process` | yes |
| `block` | a synchronous wait (`Sleep`, `Application.Wait`, a modal answer) | no | yes |
| `clock`, `identity`, `locale`, `codepage` | values that differ by machine; CalcVBA's host and model classes | no | names only |
| `codegen` | allocating executable memory (the JIT) | no | yes |

Finding for the authority axis: four of the new names (`ui.dialog`, `ui.filedialog`, `clipboard`,
`print`) are outside reaches that the ten Guarded names do not cover. They belong in the shared
list (bead dc-enu66).

The skin's `Capability` enum (`Workbook`, `Vba`, `NativeAddins`, `ComRtd`, `NativeFileDialogs`,
`ExcelClipboardFormats`, `Printing`) is the same idea at coarser grain, for a skin deciding what
to show. It should be derived from the host profile rather than listed by hand: `NativeAddins` is
`native.code:xll`, `ComRtd` is `com.os`, and so on. One table, two granularities.

### 1.3 One declaration per member

Every member of every face carries `{ reach: [...], needs: [...] }`:

| Face | Members | Where the declaration lives | State |
|---|---|---|---|
| Excel object model (VBA route, COM projection, Office.js) | 3,866 descriptor members | `descriptor/reach.json`, plus a `needs` overlay beside it, same loader, same failure on an unknown member | reach: 28 members. needs: none |
| C API commands and functions (`xlc*`, `xlf*`, `xl*`) | C_API_COMMANDS.md inventory | the `xlc` section of the same overlays (today: `xlcExec`), with an `xlf` section | 1 entry |
| Worksheet functions with host reach | section 7.3 | calc-catalogue function entries | none |
| VBA library, statements, language features | 214 rows | CalcVBA `calcvba-symbol/src/portability/matrix.rs`; its capability table for triage | per-host verdicts written by hand |
| COM classes offered or vouched | section 5 | the host's class table (CODE_EXECUTION_POLICY 4.2), each class with its members' reach | Dictionary, RegExp |
| Declare entry points | section 6 | CalcVBA's emulated entry-point table (dc-8svfs.7) | none |

So yes: it belongs in the existing descriptor core, as a second overlay with the same shape as
`reach.json`. CalcVBA gets host-member needs the way CODE_EXECUTION_POLICY question 8 proposes
for reach: a side table keyed by DISPID with the host's type information.

Two rules keep the declarations small:

1. **Members declare needs. Hosts declare provisions. Verdicts are computed.** CalcVBA's matrix
   today stores five verdicts per feature (214 x 5 cells written by hand). With seven hosts that
   does not scale and cannot be checked. A feature says `needs: fs, path.windows`; a profile says
   `path.windows: shim`; the cell follows. The TSVs in `portable-execution/` are that product,
   computed by `tools/corpus-vba-census/footprint_report.py` from one row per feature.
2. **Unmarked means pure.** As with reach: a member with no `needs` entry needs only the workbook
   and the engine (most of the model: `Range.Value`, `ChartObjects.Add`). The descriptor review
   marks the rest. Under `strict` mode an unreviewed member is reported as unreviewed.

## 2. Hosts as capability profiles

### 2.1 The hosts

| Host | What it is | Notes |
|---|---|---|
| **Windows** | DNA Calc on Windows, Excel not required | OS COM, PE DLLs, registry, MAPI exist. Other Office programs may not be installed: 429 arises naturally |
| **macOS** | desktop | POSIX files, default volume case-insensitive, AppleScript, sandbox and privacy prompts for files outside the container |
| **Linux** | desktop (WebKitGTK shell or `dnacalc serve`, LINUX_DESKTOP_SHELL.md) | POSIX files, case-sensitive, `xdg-open`, CUPS, a clipboard |
| **server** | headless CLI or service: `dnacalc run`, a job, CalcDb | no UI, no clipboard; files and network as the service account |
| **web-hosted** | our server runs the engine (native or wasm) and the browser is the skin | the file system and network are the *server's*: per-session storage, server egress. Dialogs travel to the page. The engine thread can block, so VBA stays synchronous without browser support |
| **web-wasm** | the engine is downloaded and runs in the page's Worker | no processes, no native code, no JIT. Details below |
| **WASI** | wasm under a WASI runtime | preopened directories only, clocks, stdio; no window system; sockets only where the runtime has them |

The owner's suspicion is right: web and wasm are not one host. *Web-hosted* is a server profile
with a remote UI. *Web-wasm* is a sandbox profile. They are incomparable (2.3).

**Web-wasm, concretely.**

- **Files.** The origin-private file system (OPFS) gives a private tree. In a dedicated Worker it
  has synchronous access handles, which fit VBA's synchronous `Open`/`Print #`/`Close` directly.
  The person's real files come only through the File System Access API (a picker that needs a
  user gesture on the page, Chromium browsers) or through upload and download. So a private drive
  is `shim`; the person's disk is `mediated`.
- **Network.** `fetch` under CORS: the target server must allow our origin. Some request headers
  cannot be set (`User-Agent`, `Cookie`, `Host`). No proxy settings, no client certificates. A
  synchronous request is possible inside a Worker (synchronous `XMLHttpRequest` is permitted
  there). So `WinHttpRequest.Send` is `mediated`: it works for servers that allow it.
- **Blocking.** VBA is synchronous and the main thread cannot block, so the engine runs in a
  Worker (it does today: `dna-calc-worker`). A modal `MsgBox` needs the Worker to wait for the
  page. `Atomics.wait` on a `SharedArrayBuffer` does that, and needs the page to be cross-origin
  isolated (COOP `same-origin` and COEP `require-corp` or `credentialless` response headers).
  Where isolation is not possible, stack switching (WebAssembly JSPI) is the alternative where the
  browser has it. Without either, modal UI is `reject` and the report says so (dc-8svfs.14).
- **Time.** `Date.now`, clamped timer resolution, the browser's time zone.
- **Storage.** Settings in IndexedDB. OPFS and IndexedDB are subject to quota and eviction unless
  the page asks for persistence.
- **Never:** `Shell`, Declares into DLLs, OS COM, XLLs, `SendKeys` to other windows.

### 2.2 The profile table

How each host provides each need. This table is the whole portability configuration; every
per-member verdict follows from it.

| Need | Windows | macOS | Linux | server | web-hosted | web-wasm | WASI |
|---|---|---|---|---|---|---|---|
| `fs` (some file tree) | native | native | native | native | native (session store) | shim (OPFS) | native (preopens) |
| `fs` on the person's own disk | native | native | native | native | mediated (upload, download) | mediated (picker) | native (preopens) |
| `path.windows` | native | shim | shim | shim | shim | shim | shim |
| `fs.attributes` | native | shim | shim | shim | shim | shim | shim |
| `env` (Windows names) | native | shim | shim | shim | shim | shim | shim |
| `settings` | native (registry) | shim (file) | shim (file) | shim (file) | shim (session store) | shim (IndexedDB) | shim (file) |
| `process` (Windows command lines) | native | reject | reject | reject | reject | reject | reject |
| `native.code:dll`, `:xll` | native | reject | reject | reject | reject | reject | reject |
| `native.code:dylib` | reject | native | reject | reject | reject | reject | reject |
| `com.os` | native | reject | reject | reject | reject | reject | reject |
| portable classes (section 5) | replace or native | replace | replace | replace | replace | replace | replace |
| `network` | native | native | native | native | native (server egress, policy) | mediated (CORS) | runtime-dependent |
| `ui.dialog` | host | host | host | host (answers file) or reject | host (page) | host (needs `block`) | host (stdio) or reject |
| `ui.forms` | native (MSForms) | replace | replace | reject | replace | replace | reject |
| `ui.filedialog` | host | host | host | reject | host (upload, download) | mediated (picker) | reject |
| `clipboard` | host | host | host | model (internal only) | mediated | mediated | model |
| `print` | host | host | host | replace (PDF file) | host (PDF to the page) | host | reject |
| `open.external` | native | native | native | reject | mediated | mediated | reject |
| `block` | native | native | native | native | native | shim (isolation or JSPI) | native |
| `ui.keys` (other windows) | native | reject | reject | reject | reject | reject | reject |
| `codegen` | native | native (entitlement) | native | native | native or none | none (VM3) | none (VM3) |
| `clock`, `identity`, `locale`, `codepage` | host / model | host / model | host / model | host / model | host / model | host / model | host / model |

### 2.3 Profiles form a lattice, not a ladder

For one need, the ways of providing it are ordered by how much of Excel-on-Windows they give:

```text
reject  <  mediated  <  replace  <  shim  <  host  <  model  <  native
```

A profile is a function from needs to that chain, so profiles are ordered pointwise and form a
lattice (a product of chains). What follows from that:

- **Windows is the top** for Windows-dialect code. **The meet of all profiles** (pointwise
  minimum) is the portable core: what runs everywhere. Its `reject` rows are `process`,
  `native.code`, `com.os`, `ui.keys`.
- **It is not a chain.** `server` has files and no UI; `web-wasm` has UI and no free files.
  Neither is above the other, so there is no single "portability level" a workbook can carry. A
  workbook carries a **footprint** (a set of needs) and gets one verdict *per host*.
- **A footprint F runs on host H** iff no need in F is `reject` in H. Its badge on H is the
  minimum over F of H's provisions. The hosts where F runs are an upward-closed set: if it runs on
  a profile it runs on every profile above it. That is what makes a per-host test meaningful for
  hosts not tested (a deployment with more mounts or more grants is above the tested one).
- **Guarded is the same lattice.** A grant set is a profile restricted further: Guarded with no
  grants is the profile where every outside need is `reject`. "Can it run on WASI" and "can it run
  Guarded with a folder grant" are the same kind of question, answered by the same check. The
  object-capability reading is in section 8.

## 3. Verdicts: one vocabulary

For a (member, host) pair:

| Verdict | Meaning | Example |
|---|---|---|
| `native` | The host's own facility gives the dialect's answer | `Open` on Windows; `Range.Value` anywhere |
| `model` | Our own model, the same code on every host | locale, code page, maths (CalcVBA); `Application.OperatingSystem` text |
| `host` | A machine value or the UI host's answer; the shape is portable | `Now`, `MsgBox`, `Application.StatusBar` |
| `shim` | The facility exists with a different shape and is emulated | path map, `Environ`, settings store, attribute store |
| `replace` | Only through a portable replacement we wrote | `Scripting.FileSystemObject` in Rust, `Sleep` |
| `mediated` | Exists here only through a permission of the platform or the person | a file picker in the browser, `fetch` under CORS, opening a URL |
| `reject` | Cannot work here. Typed `NotAvailableHere { capability, route }` | `Shell "cmd /c ..."` on Linux, an XLL in the browser |

Reconciling the words in use:

| Source | Its word | Here |
|---|---|---|
| CalcVBA matrix | native, model, shim, host, replace, reject | the same six, unchanged |
| Owner (dc-54aki) | native; emulated/shim; mediated; replaced; refused | native; shim; mediated; replace; reject |
| CODE_EXECUTION_POLICY 2.3 (vouched classes) | pure; projected; refused | native (the OS class, on Windows); replace; reject. A *projected* class is a replacement chosen for authority, a *portable* class is one chosen for availability. It is one class table |
| Skin `Refusal` | `NotAvailableHere { capability, route }` | the run-time form of `reject` |
| Skin `Refusal` | `NotModelled { owner, what }` | **not a verdict.** "Not here" is a property of the host. "Not yet" is a gap in the product with an owning bead. Reports keep them apart |
| Skin `Refusal` | `PolicyRefused` | the other axis |

Two columns go with every verdict in the data: `reach` (so the Guarded answer can be read off the
same row) and the state today (exists, missing, refused), because a verdict is the design and the
state is the build.

**When a `reject` is delivered** (refining CalcVBA's "rejected at load, never a run-time
surprise", which would stop a chart macro because another procedure in the project writes a file):

| Mode | Unit | Behaviour |
|---|---|---|
| `faithful` (default) | the call | The load-time report lists every reject. A procedure whose *must*-reach (section 8) holds a reject is refused when invoked, before it starts, so nothing is half done. Otherwise it runs, and a rejected call raises Excel's error for the thing being absent |
| `strict` (CI, servers) | the project | Any reject, and anything not `native` or `model`, refuses the project at load |
| `native` | none | Code written for this host; no dialect emulation |

## 4. Paths

### 4.1 What the macro sees: the Windows persona

A workbook from Windows Excel runs with the Windows dialect (CalcVBA section 5). The corpus
supports that as the default: 378 distinct projects hold a backslash path literal and 200 a
drive-letter literal; 2 hold a POSIX or Mac literal (clean and suspect sets together, static
scan, `corpus-features.tsv`).

So, by default, on every host:

| Member | Value under the Windows persona |
|---|---|
| `Application.PathSeparator` | `\` |
| `ThisWorkbook.Path`, `.FullName`, `ActiveWorkbook.*` | the persona form of the file's location, e.g. `C:\Users\maria\Documents\Budget` |
| `CurDir`, `CurDir("D")` | per-drive current directory, kept by the session; starts at `Application.DefaultFilePath` as in Excel |
| `Application.DefaultFilePath` | `C:\Users\<user>\Documents` |
| `Environ("TEMP")`, `("USERPROFILE")`, ... | persona paths (4.5) |
| `Application.OperatingSystem` | `Windows (64-bit) NT 10.00` (implemented, dc-oxh8w.39) |
| `#If Mac` / `Win64` / `VBA7` | False / True / True |
| `CELL("filename")`, `INFO("directory")` | persona form (not modelled today, section 7.3) |

The Mac dialect has its own persona and history. Mac Excel 2011 used HFS paths
(`Macintosh HD:Users:maria:Budget.xlsm`) and `PathSeparator` was `:`. Mac Excel 2016 and later is
sandboxed and uses POSIX paths with `/`; `MacScript` gave way to `AppleScriptTask`, and file
access outside the container asks the person (`GrantAccessToMultipleFiles`). Old Mac code tests
`Application.PathSeparator` or `InStr(path, ":")`. The Mac dialect is dc-8svfs.12; its values
come from Excel for Mac (dc-8svfs.11), not from this document.

### 4.2 Resolve at the facet, never rewrite the string

The owner's example: a macro writes `c:\Temp\MyFile` and the host maps it to
`/home/<user>/tmp/MyFile`. The mapping happens **inside the file facet**, when a path is used to
reach a file. The string is never changed:

- `Len`, `InStrRev(p, "\")`, `Split(p, "\")`, `Dir(p & "\*.csv")` all see Windows text and work
  as written.
- No heuristic decides which strings are paths. (MSYS rewrites command-line arguments that look
  like paths and is the cautionary case: it corrupts arguments that only look like paths.)
- A path written into a cell is the persona text. It is workbook data like any other.

### 4.3 The mount table

A per-session namespace, in the sense of Plan 9's per-process namespaces: an ordered list of
bindings from Windows path prefixes to host directories. Wine's `dosdevices` (`c:` to `drive_c`,
`z:` to `/`) is the precedent on the same problem.

```toml
# ~/.config/dnacalc/mounts.toml (host defaults; a workbook session may add or remove)
[[mount]]  windows = 'C:\'                          host = '~/.local/share/dnacalc/drive_c'   # private, created on first use
[[mount]]  windows = 'C:\Users\{user}'              host = '~'
[[mount]]  windows = 'C:\Users\{user}\Documents'    host = '{xdg:DOCUMENTS}'
[[mount]]  windows = 'C:\Users\{user}\Desktop'      host = '{xdg:DESKTOP}'
[[mount]]  windows = 'C:\Users\{user}\AppData\Local\Temp'  host = '{runtime}/dnacalc/tmp'
[[mount]]  windows = 'C:\Temp'                      host = '~/tmp'                             # the owner's example
[[mount]]  windows = '\\fileserver\finance'         host = '/mnt/finance'  mode = 'read'       # UNC only when named
# [[mount]] windows = 'Z:\'  host = '/'                                                        # Wine style; never a default
```

Rules:

1. **Longest prefix wins**, compared case-insensitively. `C:\Temp\MyFile` resolves to
   `~/tmp/MyFile`; `C:\Windows\win.ini` resolves into the private drive, where it does not exist
   (error 53, as on a Windows machine without that file).
2. **`C:` is a private directory, never `/`.** A macro that writes `C:\out.txt` must not write to
   the root of the host. This is CalcVBA's default too.
3. **The workbook's own folder is always mounted** while the workbook is open, because opening it
   was the person's act. If the folder lies under a mount, that mount gives the persona path.
   Otherwise the session binds it at a stable synthetic path (`C:\Users\{user}\Workbooks\<name>`).
   This is the mount that makes `ThisWorkbook.Path & "\data.csv"` work.
4. **Unmapped means absent.** A drive with no mount: `ChDrive "Q"` raises 68 (Device unavailable),
   `Open "Q:\x"` raises 76 (Path not found). A UNC share not named in the table: Excel's error for
   an unreachable share. The exact numbers per statement are a Windows capture (bead dc-2r0xe);
   until then the implementation uses CalcVBA's existing numbers and the cases are marked
   unmeasured.
5. **Modes.** A mount is `read` or `read-write`. Under Guarded, the grant *is* the mount list: no
   file grant means an empty table (every path is absent or denied), "this workbook's folder,
   read only" means exactly one read mount. One mechanism serves both axes.
6. **Per host.** WASI: mounts are the preopened directories, nothing else exists. Web-wasm: `C:`
   is an OPFS directory; a picked folder becomes a mount for the session. Web-hosted: mounts point
   into the tenant's session store, never the server's own tree. Windows: the identity table (no
   mapping), unless Guarded narrows it.

### 4.4 Resolving one path

Parsing follows Win32, because the macro was written against Win32:

| Form | Example | Handling |
|---|---|---|
| Drive-absolute | `C:\a\b.txt`, `c:/a/b.txt` | both separators accepted; mount lookup |
| Drive-relative | `C:b.txt` | relative to that drive's current directory (kept per drive) |
| Rooted | `\a\b.txt` | on the current drive |
| Relative | `data\b.txt`, `.\b.txt`, `..\b.txt` | from `CurDir`; `..` is resolved in the Windows path before mount lookup, so it cannot leave a mount through the host's `..` |
| UNC | `\\server\share\a` | a mount whose prefix is `\\server\share`, else absent |
| Device and long forms | `\\.\C:`, `\\?\C:\a`, `\\.\pipe\x`, `file.txt:stream` | reject |
| Reserved names | `NUL`, `CON`, `PRN`, `COM1` | `NUL` to the host's null device; the rest reject |

Then, per component, on a case-sensitive host:

- **Lookup is case-insensitive, creation is case-preserving.** An exact match wins; otherwise the
  directory is scanned for a name equal under NTFS upper-casing. Two host names that differ only
  in case make the lookup ambiguous: error, naming both (CalcVBA's decision). On Linux file
  systems with per-directory case folding the private drive can be created case-folded, which
  makes the scan unnecessary there.
- **Trailing dots and spaces** are dropped as Win32 does; the characters `< > : " | ? *` are
  refused in a component (error 52 or 75 as Excel gives).
- **`Dir` patterns** follow Windows matching (`*.*` matches names without a dot), and **`Dir`
  order is sorted** by upper-cased name as NTFS returns it. ext4 returns hash order; a macro that
  takes "the first file" depends on this.
- **Short names.** None are generated. `GetShortPathName` returns the long path, as on a Windows
  volume with 8.3 names disabled, and `Dir("*.xls")` does not also match `.xlsx`.
- **Attributes.** Read-only maps to the write permission, directory is native, hidden, system and
  archive live in the attribute store (extended attributes or a sidecar). New files get the
  archive bit, so `GetAttr` returns 32 for an ordinary file as on Windows.
- **Text.** `Print #` writes CRLF and the project's code page on every host (dc-8svfs.5).
- **Time.** `FileDateTime` is local time in the persona's zone.

The reverse direction (host path to persona path) is needed wherever the host produces a path:
`ThisWorkbook.Path`, `CurDir`, a file dialog's answer, `Dir` (names only), `CELL("filename")`.
It is the longest host-prefix match over the same table, so a round trip returns the same string.

### 4.5 Environ

`Environ` is `env` for Guarded and `env.windows` for portability (dc-8svfs.6). Names by distinct
projects in the corpus (`corpus-environ.tsv`): TEMP 32, USERNAME 27, USERPROFILE 15,
COMPUTERNAME 15, APPDATA 8, LOCALAPPDATA 7, PROGRAMFILES 5, TMP 4, PATH 3, ONEDRIVE 3; 12
projects pass a computed name.

| Group | Value off Windows |
|---|---|
| `TEMP`, `TMP`, `USERPROFILE`, `HOMEDRIVE`, `HOMEPATH`, `APPDATA`, `LOCALAPPDATA`, `PUBLIC` | persona paths, each backed by a mount |
| `USERNAME`, `COMPUTERNAME`, `USERDOMAIN` | the host's user and machine names |
| `OS`, `PROCESSOR_ARCHITECTURE`, `NUMBER_OF_PROCESSORS` | `Windows_NT`, `AMD64`, the real count |
| `SYSTEMROOT`, `WINDIR`, `PROGRAMFILES`, `COMSPEC`, `PATH` | persona paths in the private drive; nothing is there, so `Dir` finds nothing and `Shell` is rejected anyway |
| `ONEDRIVE` and similar | unset, as on a Windows machine without it |
| Host variables (`HOME`, a project's own such as `SKLEARN_VBA_EXE`) | unset unless the host configuration passes them through by name |

Names compare case-insensitively and `Environ(n)` follows VBA's 1-based order.

### 4.6 How a path round-trips

- **Into a cell and back.** `Range("A1") = ThisWorkbook.Path` stores the persona text. Reading it
  later in the same session resolves to the same folder. Saved and opened on Windows, it is a
  Windows-looking path that may not exist there. That is what happens between two Windows
  machines too; nothing new.
- **Formulas and the file.** External links are stored in the package as the file gives them
  (relative when the target is near the workbook, absolute otherwise). We never rewrite them:
  the file is the file. At load the link target resolves through the mount table.
- **A path the person picked.** A file dialog returns a host path; the facet returns its persona
  form. If no mount covers it, the pick itself creates a session mount (the pick is the grant), so
  the returned path resolves.
- **`Shell` and Declares** would receive persona paths. Both are rejected off Windows, so no
  persona path ever reaches host code that would misread it. A replaced entry point
  (`SetCurrentDirectoryA`, `ShellExecute "open"`) takes the path through the facet.

### 4.7 Refused

Device paths, pipes, alternate data streams, raw volumes, drives and shares with no mount, the
host's own root as a default mount, and in every mode: writing through a `read` mount (error 75).

## 5. COM, pragmatically

### 5.1 What VBA needs from COM, and what it does not

| Needed by VBA code | Provided by |
|---|---|
| Find a class by ProgID or by `New Lib.Class` | the class table (below) |
| Bind members at compile time for early-bound code | type information: the descriptor of the library |
| Invoke by DISPID, named arguments, default members, `For Each`, properties with arguments | the Automation call protocol CalcVBA already speaks to the host object model |
| `WithEvents`, `Is`, `TypeName`, `Nothing`, error information | the same protocol |

Not needed, and not built: class factories and the registry, apartments, marshalling and proxies,
monikers, the running object table, DCOM. Those exist to cross process and machine boundaries. A
portable class is an in-process Rust object behind the same trait the host's `Application` is
behind (`ObjectModel::invoke`), with a descriptor for its type information. API_FACES 3.4 plans a
binary COM ABI on Linux for native *callers* of DNA Calc; that is a different direction and VBA
does not need it.

### 5.2 Two tables

**The class table** is CODE_EXECUTION_POLICY 4.2's *offered classes*, keyed by CLSID, with the
ProgIDs each answers to. Class resolution (4.3 there) is unchanged: ProgID to CLSID through the
host's table first, then, on Windows only, the OS registry. Off Windows the *vouched* table is
empty and the offered table carries everything.

Classes by distinct corpus projects (clean + suspect, late or early bound; the full table with a
verdict per host is `corpus-com-classes.tsv`):

| Class | Projects | Reach | Off Windows | State today |
|---|---:|---|---|---|
| `Scripting.Dictionary` | 138 | none | replace | exists |
| `Scripting.FileSystemObject` | 136 | fs.read, fs.write | replace, over the file facet and path map | missing |
| `WScript.Shell` | 71 | process, settings, env | replace **in part**: `SpecialFolders`, `ExpandEnvironmentStrings`, `CurrentDirectory`, `Popup`. `Run`, `Exec`, `RegRead`, `RegWrite` reject | missing |
| `ADODB.Stream` | 62 | fs.read, fs.write | replace: bytes, charsets, `LoadFromFile`, `SaveToFile` | missing |
| `VBScript.RegExp` | 61 | none | replace | exists |
| `MSXML2.XMLHTTP`, `ServerXMLHTTP`, `WinHttp.WinHttpRequest` | 31 + 13 + 25 | network | replace over the network facet; mediated in web-wasm | missing |
| `MSXML2.DOMDocument` | 29 | fs.read, network (`load`, `save`, external entities) | replace: DOM and XPath | missing |
| `Outlook.Application` | 29 | another program | reject | refused |
| `Shell.Application` | 25 | process, files | reject. Two idioms could be replaced later: `BrowseForFolder`, zip through `Namespace().CopyHere` | refused |
| a ProgID computed at run time | 25 | unknown until run | resolved at the call | |
| `htmlfile` (MSHTML) | 18 | network, script | reject | refused |
| `Word.Application` | 17 | another program | reject | refused |
| `ADODB.Connection`, `Recordset`, `Command` | 16 + 14 + 5 | OLE DB providers | reject. A disconnected in-memory `Recordset` could be replaced later | refused |
| `MSForms.DataObject` (the clipboard idiom) | 15 | clipboard | replace over the clipboard facet | missing |
| WMI (`winmgmts:` monikers, `WbemScripting`) | 15 | machine management | reject | refused |
| mscorlib classes (`ArrayList`, `StringBuilder`, hash classes) | 14 | none | replace; on Windows they need .NET Framework 3.5 | missing |
| `Scriptlet.TypeLib` (the GUID idiom), `WScript.Network` | 5 + 3 | none / identity | replace | missing |
| CDO mail, Internet Explorer, PowerPoint, Access, Excel, WIA, MSXML SAX, unlisted classes | 39 | | reject | refused |

Build order by this table: FileSystemObject, ADODB.Stream, the HTTP classes on the network facet,
MSXML DOM, the WScript.Shell subset, DataObject (dc-8svfs.8 carries it).

Each replacement is checked against the Windows class by probes, as CalcVBA does for Dictionary
and RegExp. Where Excel-on-Windows behaviour is the OS class's behaviour, the OS class is the
reference.

**The type-library table** is keyed by the library GUID that `VBProject.References` stores
(`corpus-vba-census`: Scripting in 86 projects, VBIDE 36, MSXML2 31, ADODB 20,
VBScript_RegExp_55 18, WinHttp 18, MSHTML 8, IWshRuntimeLibrary 8, Word 7, Outlook 7). Each entry
has one of three states:

| State | Compile | `New`, `CreateObject` | Example off Windows |
|---|---|---|---|
| **implemented** | binds against our descriptor | our class | Scripting, VBScript_RegExp_55 |
| **types only** | binds against the descriptor (extracted on Windows by `tools/com-descriptor`, as Excel's and Office's are) | 429 | Word, Outlook, ADODB until replaced |
| **unknown** | the reference is missing | 429 | a third-party control |

### 5.3 Lazy resolution is Excel's own behaviour

"A missing reference fails only where used" needs care, because Excel's behaviour for a reference
marked MISSING in the References dialog is harsher: the project stops compiling, and it is
commonly seen as unrelated functions failing ("Can't find project or library").

The state we reproduce is a different one, also defined by Excel: **the type library is present
and the server is not** (Outlook's type library registered, Outlook removed or broken). Then the
project compiles, every procedure that does not create the object runs, and `New
Outlook.Application` or `CreateObject` raises 429 at that line. A *types only* entry is exactly
that machine state. So for every library we hold a descriptor of, lazy failure is Excel's
behaviour.

For an *unknown* library there is no Excel behaviour worth copying. The proposal: modules that
name none of its types compile and run; a procedure that names one is refused when invoked, with
the library named. That is kinder than VBE and is the one place this design chooses to differ
from Excel-with-a-missing-reference. It is the owner's call, and the exact Excel errors for both
states are a Windows capture (bead dc-2r0xe).

### 5.4 GetObject, and Windows

- `GetObject(path)` for a workbook path is `Workbooks.Open` through the facet (origin (a)). For
  any other file it needs a moniker and a server: reject, 432 or 429 as Excel gives.
- `GetObject(, "Excel.Application")`: CODE_EXECUTION_POLICY open question 7 (return the host's
  own `Application`). `GetObject("winmgmts:...")`: reject.
- **On Windows, real COM exists.** Unguarded: the OS classes are used, because Excel's behaviour
  there *is* those DLLs. Guarded: pure vouched classes as today, replacements for classes with
  reach. A `--com portable` switch forces the replacement table on Windows too, so one machine
  can compare the OS class with the replacement, and a workbook can be checked for the other
  hosts.

## 6. Declare statements

### 6.1 Excel's behaviour is already lazy

A `Declare` compiles without the library being present. The library is loaded at the first call:
error 53 (File not found) when the DLL is absent, 453 when the entry point is. So the portable
rule needs no new behaviour: **an entry point with no emulation raises 53 at its first call**,
and the load-time report lists it.

- **`PtrSafe`, `LongPtr`.** The persona is 64-bit Excel on every host: `Win64` and `VBA7` True,
  `LongPtr` 64 bits, including on wasm32 (CalcVBA's decision). 148 of the 175 projects that use
  `Declare` already carry `PtrSafe`. A Declare without `PtrSafe` is a compile error, as in 64-bit
  Excel.
- **Handles** returned by emulated entry points are opaque 64-bit tokens from a session table,
  never host addresses.
- **`A` and `W` entry points.** The `A` form converts through the code page model.
- **`VarPtr`, `StrPtr`, `RtlMoveMemory`** work on the VM's own memory. They are portable and they
  defeat containment, so Guarded refuses them (CODE_EXECUTION_POLICY question 5). Portability and
  authority disagree here, which is the point of having two axes.
- **The Mac dylib Declare** (`Declare PtrSafe Function popen Lib "libc.dylib" ...`). 62 projects
  carry `#If Mac` branches. A first scan that read both branches found a dylib Declare in 46
  projects, in 32 of them the same four (`popen`, `pclose`, `fread`, `feof`: run a shell command
  and read its output). With the Windows dialect that branch is dead and adds nothing to the
  footprint: after evaluating `#If`, 2 projects still reach a dylib Declare. Under
  the Mac dialect on macOS it is `native` (and `process`, so Unguarded only); on every other host
  `reject`.

### 6.2 The shim table, by corpus frequency

736 distinct entry points in 175 projects (7 triage-clean, 168 suspect; `corpus-declares.tsv`
lists 261 with a verdict per host). By group, with project-uses summed over the group's entry
points:

| Group | Entry points | Uses | Off Windows | Most used |
|---|---:|---:|---|---|
| **time** | 15 | 212 | replace | `Sleep` 49, `GetTimeZoneInformation` 37, `SystemTimeToTzSpecificLocalTime` 35, `TzSpecificLocalTimeToSystemTime` 35, `QueryPerformanceCounter` 11, `QueryPerformanceFrequency` 11, `GetLastError` 10, `GetSystemTime` 7, `GetTickCount` 6 |
| **memory** | 11 | 124 | replace over VM memory; Guarded refuses | `RtlMoveMemory` 52, `GlobalAlloc` 13, `GlobalLock` 11, `GlobalUnlock` 11, `CoTaskMemFree` 9 |
| **string** | 15 | 69 | replace | `lstrlenW` 11, `WideCharToMultiByte` 10, `MultiByteToWideChar` 6, `SysAllocString` 6 |
| **clipboard** | 8 | 54 | replace over the clipboard facet; mediated on the web | `OpenClipboard`, `EmptyClipboard`, `SetClipboardData`, `CloseClipboard` 10 each |
| **path** | 20 | 45 | replace through the path map | `SetCurrentDirectoryA` 6, `FindFirstFile*`, `GetShortPathNameW` 3, `GetPrivateProfileStringA` 3 |
| **identity** | 11 | 27 | replace from the persona | `GetSystemMetrics` 4, `GetEnvironmentVariableW` 3, `GetCurrentThreadId` 3 |
| **keys** | 3 | 19 | replace: key state of our own window | `GetAsyncKeyState` 10 |
| **guid** | 4 | 15 | replace | `CoCreateGuid` 5, `IIDFromString` 5 |
| **dialog** | 4 | 10 | replace through the file dialog facet | `ChooseColorA` 5, `SHBrowseForFolderA` 2 |
| **open** | 1 | 8 | mediated: the `open` verb on a document or URL | `ShellExecuteA` 8 |
| **download** | 1 | 3 | replace over network and file facets | `URLDownloadToFileA` 3 |
| window | 113 | 333 | reject: needs an HWND | `FindWindowA` 18, `SetWindowPos` 11, `SendMessageA` 11, `GetWindowLong*`, `SetWindowLong*` |
| own or third-party DLL | 274 | 284 | reject: `native.code` | add-in helper DLLs shipped beside the workbook |
| gdi | 81 | 169 | reject: needs a device context | `GetDC` 12, `GetDeviceCaps` 9 (the DPI idiom) |
| code | 23 | 124 | reject: native code, memory protection, processes | `DispCallFunc` 15, `GetModuleHandleA` 14, `LoadLibraryA` 11, `VirtualAlloc` 9 |
| net | 53 | 116 | reject: use the `WinHttpRequest` class | `WinHttpOpen` 9 |
| handle | 6 | 41 | reject: raw kernel handles | `CloseHandle` 14, `WriteFile` 8 |
| registry, other | 88 | 97 | reject | |

How far the table goes, counted over the 175 projects (a project is covered when every entry
point it declares is emulated):

| Table built so far | Projects fully covered |
|---|---:|
| tier 1: time, string, identity, guid, path | 40 |
| plus memory | 51 |
| plus tier 2: clipboard, keys, open, download | 66 |
| plus tier 3: dialog | 68 (39%) |

The rest are blocked by window functions (54 projects), native code (43), their own DLLs (27),
GDI (24), other (24), raw handles (20) and raw network calls (14). Because failure is per call,
the procedures of those projects that never reach the blocked entry point still run.

The owner's examples: `Sleep` and `GetTickCount` are tier 1. `GetUserNameA` is tier 1 (identity),
though no corpus project declares it; the corpus reads the user through `Environ("USERNAME")` (27
projects) and `Application.UserName`. `ShellExecute` is tier 2 and `mediated`: only the `open`
verb, only a document or URL, through the same facet as `FollowHyperlink`.

Window-chrome entry points (`FindWindow`, `SetWindowLong` on a UserForm) are not given empty
implementations. A function that returns "no such window" would be truthful, and code that then
styles the missing window would fail later and less clearly. They stay `reject` until the forms
runtime can offer real handles for its own windows.

## 7. The C API, XLLs and worksheet functions

### 7.1 The native-binary question

An XLL is a Windows x64 DLL. Its *source* is written against `xlcall.h`. Three options:

| | Option | Position |
|---|---|---|
| A | **Refuse foreign binaries.** A PE XLL off Windows is `NotAvailableHere { NativeAddins, route: "DNA Calc on Windows" }` | Today's rule (LINUX_DESKTOP_SHELL.md). Keep it. No emulation layer for PE |
| B | **Source-compatible XLL.** The same exports (`xlAutoOpen`, registered functions) and the same `Excel12` callback, built as an ELF or Mach-O library | Possible: `calc-xll-abi` already compiles and passes its layout tests on Linux, and API_FACES 4.4 plans the in-process C API dispatcher there. It needs a clean header, because `xlcall.h` leans on `windows.h` types whose sizes differ (`long` is 32 bits on Win64 and 64 on Linux; `XCHAR` must stay 16 bits where `wchar_t` is 32). It is a packaging Excel does not have (Excel for Mac never loaded XLLs), so by "the engine does no more than Excel" it needs the owner's decision. Unguarded only |
| C | **XLL compiled to wasm.** The same source built for wasm32, loaded as a module whose only imports are the C API callbacks | The interesting one. A wasm module has no ambient authority: it can reach only what it imports. That is containment by construction, the thing GUARDED_EXECUTION 4.5 says native add-ins lack. It would run on every host including the browser, **and under Guarded**. Same owner decision as B, with a much better safety story |

Recommendation: A stays the default answer for existing binaries. B and C are recorded as a
decision for the owner (bead dc-ljnou), with C preferred if either is taken. Managed (Excel-DNA)
add-ins are a separate question and are not designed here.

### 7.2 C API members with host reach

Most of the C API is workbook work and is portable through the dispatcher. These reach the host:

| Function or command | Reach / need | Off Windows |
|---|---|---|
| `xlfRegister`, `xlfUnregister`, `xlfCall`, `xlfRegisterId` (and worksheet `CALL`, `REGISTER.ID`) | ffi, `native.code` | reject (or B/C above) |
| `xlGetName` (the XLL's path), `xlGetInst`, `xlGetInstPtr`, `xlGetHwnd` | identity of the native module and window | only inside a native add-in, so only on Windows; a B/C add-in gets a persona path and no HWND (`xlretFailed`) |
| `xlfGetWorkspace` | per info type: 1 (environment and version) model; 2 (`"16.0"`) model; 23, 24, 48 (startup and library paths) persona paths; 26 (user) host; 37 (international settings) locale model; 44 (registered procedures: DLL paths) persona; window and memory types host | model / shim |
| `xlfGetDocument` | 1, 76, 88 (names) native; 2 (the workbook's path) persona | shim |
| `xlfGetCell` 62, 66, `xlfGetWindow`, `xlfDocuments`, `xlfWindows`, `xlfLinks`, `xlfGetLinkInfo` | workbook names and link paths | native / shim |
| `xlfFiles`, `xlfDirectory`, `xlfFopen`, `xlfFclose`, `xlfFread`, `xlfFreadln`, `xlfFwrite`, `xlfFwriteln`, `xlfFpos`, `xlfFsize`, `xlcFileDelete` | fs.read, fs.write, `path.windows` | shim, the same facet as VBA |
| `xlcOpen`, `xlcSaveAs`, `xlcSave`, `xlcOpenLinks`, `xlcChangeLink`, `xlcUpdateLink` | fs, network | shim |
| `xlfExec` (`xlcExec` in reach.json), `xlcAppActivate`, `xlcSendKeys` | process, ui.keys | reject |
| `xlfInitiate`, `xlfExecute`, `xlfPoke`, `xlfRequest`, `xlfTerminate` | DDE | reject |
| `xlcAlert`, `xlcMessage`, `xlcInput`, `xlPrompt` | ui.dialog | host |
| `xlcPrint`, `xlcPageSetup`, `xlcPrinterSetup` | print | host |
| `xlcOnTime`, `xlcOnKey` | timer, own keys | host |
| `xlcSendMail` | mail | reject |
| `xlEnableXLMsgs`, `xlDisableXLMsgs`, `xlStack`, `xlAbort`, `xlAsyncReturn`, `xlEventRegister`, `xlSheetId`, `xlSheetNm`, `xlCoerce`, `xlFree`, `xlSet`, `xlUDF`, `xlDefineBinaryName`, `xlGetBinaryName` | none | native |

These rows go into the `xlc` and `xlf` sections of the overlays (bead dc-9x1fr). The info-type
values under the persona follow section 4; each is an Excel capture before it is asserted.

### 7.3 Worksheet functions and workbook features with host reach

Measured on Linux today with `dnacalc set` on a scratch workbook (binary `8afd4123...`, master
`4d2013218`):

| Formula or feature | Reach / need | Linux today | Design |
|---|---|---|---|
| `CELL("filename")`, `CELL("filename", A1)` | `path.windows` | typed not-modelled refusal | persona path: `C:\...\[Book.xlsx]Sheet1` |
| `INFO("directory")` | `path.windows` | not modelled | persona `CurDir` with a trailing `\` |
| `INFO("osversion")`, `("system")`, `("release")` | identity | not modelled | model: `Windows (64-bit) NT 10.00`, `pcdos`, `16.0` |
| `INFO("numfile")`, `("origin")` | none | calculated (`1`, `$A:$A$1`) | native |
| `HYPERLINK(target, name)` | value: none. Following it: `open.external` | calculated (the name) | native; the click is mediated |
| `WEBSERVICE(url)` | network | `#VALUE!` | the network facet; under Guarded a decision (dc-hk7im); mediated in web-wasm |
| `IMAGE(url)` | network | | as WEBSERVICE |
| `RTD(progid, ...)` | `com.os` | `#N/A` | reject off Windows: `#N/A`, which is Excel's value for a server that cannot be created |
| External workbook links | fs.read, network, `path.windows` | not modelled (dc-ylr) | resolve the stored target through the mount table |
| DDE links (`=app\|topic!item`) | DDE | | reject: Excel's value for a server that does not answer (to capture) |
| `CALL`, `REGISTER.ID` | ffi | | reject |
| Cube functions, queries, data connections | `data.connection` | | reject, except text and web queries |
| Stock and geography data types, `STOCKHISTORY`, `TRANSLATE`, `DETECTLANGUAGE`, `PY`, `COPILOT` | a Microsoft service and account | | reject on every host, Windows included: not Excel's engine but its service |
| `NOW`, `TODAY`, `RAND` | clock, entropy | calculated | host |

Formulas are code (GUARDED_EXECUTION 4.1), so a workbook's footprint is the join of its VBA
footprint and its formulas' needs. A workbook with no macro can still be `reject` on a host (an
RTD formula) or need a grant (`WEBSERVICE`). The four refusals in the table go to bead
dc-xy16l.

## 8. Static analysis and its theory

### 8.1 The analysis is an effect system

Give every procedure an **effect**: the set of needs (and reach) its execution may use. This is a
type-and-effect system in the sense of Lucassen and Gifford (1988) with capability sets as
effects. A statement's effect is the union of its parts; a call's effect includes the callee's
*latent* effect; a procedure's latent effect is the least solution of those equations over the
call graph.

- **The lattice** is the powerset of capability names ordered by inclusion: finite, so the
  fixpoint exists and iteration terminates (Kildall 1973; Kam and Ullman 1977). Solving
  strongly connected components of the call graph in reverse topological order (Tarjan) makes it
  linear in practice. It is one instance of abstract interpretation (Cousot and Cousot 1977).
- **May and must.** *May*-effect: the union over all paths. It over-approximates, so `X not in
  may(P)` proves P never needs X. That is what a "runs here" badge rests on. *Must*-effect: the
  needs used on every path from entry to exit (the effects at statements that dominate the exit).
  It under-approximates, so `X in must(P)` proves P cannot finish without X. That is what an
  up-front refusal rests on. Between them is "may stop here", and the honest badge has three
  states, not two.
- **Effect polymorphism.** A procedure that calls something it was handed has an effect that
  depends on its argument: `Application.Run name`, `CallByName obj, name`, `AddressOf`, an `Object`
  parameter with late-bound calls, `Evaluate text`. In the theory these get effect variables. In
  practice we instantiate them where the argument is a constant (next point) and write the top
  element (`unknown`) where it is not. `unknown` is reported as unknown, never as clean.
- **Constant propagation for late binding** (Kildall; Wegman and Zadeck's sparse conditional
  form, 1991). `CreateObject("Scripting.FileSystemObject")` names its class; so does a `Const` or
  a variable assigned once. In the corpus 288 projects pass `CreateObject` a literal
  ProgID and 25 compute one; `Application.Run` has a literal target in 70
  projects and a computed one in 73. For paths a small string domain is enough (string analysis,
  Christensen, Møller and Schwartzbach 2003, in a much reduced form): a value is a literal, or
  `WorkbookDir + suffix`, `Env(TEMP) + suffix`, `UserChosen`, or unknown. That domain is what lets
  a report say "writes only beside the workbook", which is also the grant Guarded should offer.
- **Objects.** `x.Save` on a typed variable binds through the descriptor. On `Object` or
  `Variant` it is name-based: every class in the project and every library member with that name
  (class hierarchy analysis, Dean, Grove and Chambers 1995; restricted to instantiated classes as
  in rapid type analysis, Bacon and Sweeney 1996).
- **Conditional compilation** is evaluated with the dialect's constants before anything else, so
  `#If Mac` code adds nothing to a Windows-dialect footprint (section 6.1: 46 projects to 2).
- **Roots.** Entry points are what the host can invoke: buttons and `OnAction` names, document
  and workbook events, UDFs called from cells, `Auto_Open`. The workbook's footprint is the join
  over roots, plus the formulas (7.3).

**Soundness, stated plainly.** Exact footprints are undecidable (Rice). The analysis
over-approximates, and it is deliberately unsound in the listed places where VBA builds code or
names at run time (the "soundiness" position, Livshits et al. 2015): `Evaluate` and
`ExecuteExcel4Macro` text, computed `Run` targets and ProgIDs, formulas written into cells,
`VBProject` edits, memory written through Declares. Each is marked `unknown` at the site. The
static report is therefore advice. **The run-time check is the guarantee:** every facet call
passes the availability check and `decide`, so a wrong report costs a surprise, never a wrong
effect. A static check before and a dynamic check at the boundary is the same split gradual
typing makes (Siek and Taha 2006).

What the first measurement shows about closure (`summary.json`): in the triage-clean set 2,875 of
16,615 procedures use a host need directly, and 4,771 do once callees are included. In the
suspect set it is 10,365 directly and 20,803 after closure, of 47,868. A per-procedure answer
without the call graph misses about four in ten.

### 8.2 Capabilities, shared with Guarded

The object-capability model (Dennis and Van Horn 1966; Miller 2006) says authority is a reference:
code can affect only what it holds a reference to, and there is no ambient authority. Guarded is
that model applied to VBA (GUARDED_EXECUTION: "the code can only reach what it was handed").
Portability uses the same structure for a different reason:

| | Object-capability reading |
|---|---|
| A facet | an unforgeable reference to a host service |
| Host profile | which facets this host can construct |
| Level and grants | which constructed facets are handed to this code (least privilege, Saltzer and Schroeder 1975) |
| Mount table | an attenuated file capability: the same facet, narrowed |
| A replacement class | a facet built from other facets (FileSystemObject from the file facet), so it inherits their limits and cannot exceed them |
| A wasm XLL (7.1 C) | native code with no ambient authority |

WASI is this model at the system-call level: a module can open files only under directories it
was given (preopens), which is why the WASI mount table is the preopen list and nothing else.
Capsicum (Watson et al. 2010) is the same idea for Unix processes. The confused-deputy problem
(Hardy 1988) is why a replacement class must act with the *caller's* facets and never the host's
own: FileSystemObject resolves through the session's mount table, not the process's file system.

### 8.3 Namespaces

Plan 9 gave each process its own name space, assembled with `bind` and `mount` (Pike et al.
1992); a program's view of files is a construction, not a fact about the machine. The mount table
is that idea at the size we need: per workbook session, ordered, longest prefix. Related practice:
Wine's `dosdevices` and its case-insensitive lookup on case-sensitive file systems (the same
problem in the same direction); WSL's `/mnt/c` and Cygwin's `/cygdrive` (the other direction);
Linux mount namespaces and `chroot`. What we take from them: the table is explicit and
inspectable, the private drive is not the host root, and translation happens at the boundary.

## 9. What runs on Linux today

### 9.1 What was measured

- **The run.** Every staged triage-clean macro workbook (3,248 of the 3,261 reconciled hashes,
  `~/corpus-macro` on dna-firehorse, the set of docs/plan/macro-host-baseline.md) through
  `dnacalc run <book> --all --fixtures --macro-level guarded --json`, one fresh Bubblewrap
  sandbox per workbook (read-only copy, no network, 120 CPU s, 4 GiB, 300 s wall), six at a time.
  Binary built from master `4d2013218` (SHA-256 `8afd4123d3e1...`), 2026-11-28. Nothing was
  opened in Office. This is the **headless CLI on Linux**, so the profile measured is `server`,
  not the Linux desktop, and the level is Guarded with no grants.
- **The scan.** `tools/corpus-vba-census/footprint.py` over the same workbooks, and over the 552
  triage-suspect workbooks copied from R2 for reading only. Suspect projects were never run.
  It reads module source statically, evaluates `#If` with the Windows constants, records
  host-reach constructs per procedure and closes them over the call graph. It is a regex scan
  with name-based calls: the limits are in 9.5.
- **Units.** A *project* is distinct by its module source (workbooks that are copies of one
  project count once). 3,245 clean workbooks with modules are 1,324 projects and 16,615
  procedures; 549 suspect workbooks are 532 projects and 47,868 procedures. An *entry* is a
  public Sub or Function the command route offers.

Against the baseline of 2026-11-25 (same set and method, docs/plan/macro-host-baseline.md,
every workbook counted): workbooks admitted 2,147 -> 2,595; entries succeeding 787 -> 3,017;
VBA run-time errors 504 -> 716; typed not-modelled refusals 1,216 -> 1,183.

### 9.2 The table the owner asked for

Entries of the 976 admitted distinct projects, by what the scan says the entry may need on Linux
(rows) and what happened (columns). 322 projects were not admitted and 26 gave no result; their
7,324 public procedures are in the last column, classified but not run.

| Linux class of the entry | Succeeded | Stopped: member not modelled | VBA run-time error | Host facility absent | Guarded denial | Other | Not run (project not admitted) |
|---|---:|---:|---:|---:|---:|---:|---:|
| **native**: needs nothing from the host | 932 | 398 | 371 | 0 | 1 | 32 | 5,182 |
| **model**: internal clipboard, `Run`, XLM | 22 | 59 | 86 | 0 | 0 | 0 | 111 |
| **host**: dialogs, clock, status bar, printing | 133 | 203 | 52 | 52 | 0 | 4 | 1,448 |
| **shim**: files, paths, `Environ`, file dialogs | 0 | 36 | 4 | 1 | 4 | 0 | 146 |
| **replace**: a portable class or the forms runtime | 17 | 6 | 24 | 0 | 0 | 1 | 405 |
| **reject**: cannot run on Linux | 0 | 2 | 0 | 0 | 9 | 0 | 32 |
| **Total** | **1,104** | **704** | **537** | **53** | **14** | **37** | **7,324** |

Read as the owner's four groups (2,449 entries run):

| Group | Entries | What it is |
|---|---:|---|
| **Runs today** | 1,104 (45.1%) | 932 need nothing, 133 use only dialogs or the clock on paths that did not block, 22 use the internal clipboard or `Run`, 17 use `VBScript.RegExp` or `Scripting.Dictionary`, which the engine already has |
| **Would run with a shim or replacement** | 45 shim + 31 replace that did not succeed | By mechanism, entries that need it (and entries that need only it): path map 40 (29), reverse path map for `ThisWorkbook.Path` 14 (5), MSForms runtime 25 (25), `VBScript.RegExp` 14 and `Scripting.Dictionary` 5 (exist), text and web queries 4 (4), persona values 2 (1). Most of them stop earlier today at a member that is not modelled (36 of the 45) |
| **Needs a grant** | 65 | Entries whose may-reach holds a Guarded capability. None succeeded; 14 were denied at run time (`ffi` 9, `fs.read` 3, `fs.write` 2), the rest stopped earlier. The 14 denials all fell on entries the scan had marked, bar one synthetic entry it could not match |
| **Refused on Linux** | 11 run + 32 not run | A Novell NetWare client DLL (`calwin32`, `clxwin32`: 8 entry points in one project), `SendMail` (2), `GetObject` of a Word document (1) |
| **Host facility absent** (this is the CLI, not the desktop) | 53 | `MsgBox` 41 and `InputBox` 12 reached in a process with no UI: CalcVBA raises error 5 (`HAL-E-CAP-UNAVAILABLE`, `UiInteraction`). A desktop host answers them; the CLI needs an answers source (bead dc-smdg7) |
| **Not portability at all** | 704 + 537 | Members not modelled yet (`Window.SelectedSheets` 87, `Window.SmallScroll` 60, `Window.ScrollWorkbookTabs` 59, `Range.Sort` custom order 36, `Range.Clear` 32) and VBA errors (1004: 208, 9: 162, 13: 96, 91: 40). Owned by dc-oxh8w and dc-v7k28 |

Refused or failed counts as fail: of 2,449 entries run, 1,104 pass. Of all 9,773 entries of the
1,324 projects, 1,104 pass (11.3%); the largest loss is admission (322 projects: compile
refusals 130, locked projects 75, unbound or invalid modules 57, others), owned by dc-cmruz,
dc-711ye, dc-oxh8w.73 and dc-1jia7.

### 9.3 The owner's two examples

- **A button that makes a chart from data runs.** `Module1.CreateAdvancedChart` in workbook
  `4185a9839060...` builds a chart and succeeds on Linux. Chart macros need nothing from the
  host: 348 procedures in 31 clean projects use chart members, and of the 9 chart entries that
  ran, 1 succeeded and 8 stopped at a chart member that is not modelled yet (`ChartObject.Select`,
  `Chart.FullSeriesCollection`, a chart type). None stopped for a portability reason.
- **A macro that does file I/O stops at the path.** `ImportarHoja.importarHoja` in `094189733a28...`
  calls `Dir("c:\libros\*.xl??")`. Guarded denies it (`fs.read`, error 70, subject
  `c:\libros\*.xl??` in the audit). `Module9.legmaxwebupdate` in `4d608771dfc5...` does `ChDir
  "T:\Home_page\legal_max"` and is denied `fs.write`. With a file grant these two reach the next
  question, which is the path map: `c:\libros` and `T:\` exist only if the mount table names
  them. Of the 40 file entries that ran, 30 stopped earlier at an unmodelled member
  (`Workbooks.Open` without a workbook factory, `GetOpenFilename`, `FileDialog`), 5 were denied,
  5 hit other errors, and none succeeded.

### 9.4 Every project, per host (static)

Projects by the worst verdict over all their procedures. This is the badge a whole workbook
would get; per procedure it is better (9.2), because failure is per call.

Triage-clean, 1,324 projects:

| Worst verdict | Windows | macOS | Linux | server | web-hosted | web-wasm | WASI |
|---|---:|---:|---:|---:|---:|---:|---:|
| native | 872 | 821 | 821 | 820 | 820 | 817 | 820 |
| model | 91 | 79 | 79 | 79 | 79 | 76 | 79 |
| host | 361 | 199 | 199 | 136 | 199 | 185 | 135 |
| shim | 0 | 67 | 67 | 38 | 67 | 8 | 34 |
| replace | 0 | 134 | 134 | 95 | 132 | 95 | 17 |
| mediated | 0 | 0 | 0 | 0 | 3 | 119 | 0 |
| reject | 0 | 24 | 24 | 156 | 24 | 24 | 239 |

Triage-suspect, 532 projects (scanned, never run):

| Worst verdict | Windows | macOS | Linux | server | web-hosted | web-wasm | WASI |
|---|---:|---:|---:|---:|---:|---:|---:|
| native | 61 | 19 | 18 | 17 | 17 | 17 | 17 |
| model | 77 | 3 | 3 | 3 | 3 | 3 | 3 |
| host | 388 | 25 | 25 | 22 | 24 | 23 | 22 |
| shim | 0 | 87 | 87 | 76 | 87 | 8 | 76 |
| replace | 0 | 193 | 193 | 92 | 182 | 94 | 85 |
| mediated | 0 | 3 | 2 | 0 | 15 | 183 | 0 |
| reject | 6 | 202 | 204 | 322 | 204 | 204 | 329 |

What it says:

- **Clean projects are mostly portable.** On a Linux desktop 1,099 of 1,324 (83.0%) need nothing
  that is not there today (native, model or host). 67 need the path map and its relatives, 134
  need a replacement (UserForms are shown in 104 projects), 24 hold something that cannot run.
- **The triage filter and the portability filter are nearly the same filter.** The suspect set
  was selected for exactly the constructs that reach the host: 46 of 532 need nothing, 204 hold a
  procedure that cannot run off Windows, and 280 would be unlocked by the path map (87) and the
  replacement classes and Declare table (193). So the work of sections 4 to 6 pays mostly on
  projects that Guarded also has to make decisions about. The two axes meet on the same code.
- **The server and WASI lose the forms** (reject 156 and 239 for clean). **Web-wasm trades shim
  for mediated**: 119 clean projects open or save a file, which in a browser is a picker or a
  download.
- **Grants.** 495 of 16,615 clean procedures (3.0%) may need a Guarded capability (`fs.write`
  280, `fs.read` 244, `network` 232, `ffi` 60, `com.activate` 21, `env` 12); in the suspect set
  14,370 of 47,868 (30.0%). This is the first static estimate for dc-a6ee2.

### 9.5 Limits of this measurement

- The scan is regular expressions over source, with every identifier equal to a procedure name
  counted as a call. It over-approximates calls and under-approximates anything spelled in a way
  it does not know. Object-model members are matched by spelling, not by receiver type.
- The run is Guarded and headless. Unguarded results on a Linux desktop (where `MsgBox` answers
  and files open) are not measured; LINUX_DESKTOP_SHELL.md lists what Unguarded should mean for
  `Shell`, `Environ` and paths as open, and section 4 is the proposal.
- Fixture arguments (`--fixtures`: Number 1, Boolean True, Text "1") decide which paths run.
- The verdict tables are this document's proposal. A `shim` or `replace` verdict says the design
  has an answer, not that the answer is built: the state column in the data says which exist.
- 13 of the 3,261 clean hashes are not staged on this host (docs/plan/macro-host-baseline.md).

## 10. Function lists

Data files in [portable-execution/](portable-execution/), all written by
`tools/corpus-vba-census/footprint_report.py`. Hosts are columns `windows macos linux server
web-hosted web-wasm wasi`. Project counts are distinct projects, triage-clean and triage-suspect
kept apart (the suspect set is scanned, never run).

| File | Rows | Content |
|---|---:|---|
| `corpus-members.tsv` | 414 | 214 VBA library members, statements and language features from CalcVBA's matrix widened to seven hosts, each with its class; then the 200 most used object-model member spellings, marked `document` or `host-reach` |
| `corpus-features.tsv` | 73 | one row per host-reach feature the scanner knows: reach, needs, verdict per host, mechanism, owning bead |
| `corpus-com-classes.tsv` | 34 | COM classes: reach, verdict per host, state off Windows today |
| `corpus-declares.tsv` | 261 | Declare entry points with their group and verdict per host |
| `corpus-environ.tsv` | 61 | `Environ` names |
| `static-class-clean.tsv`, `static-class-suspect.tsv` | 14 each | procedures and projects by worst verdict, per host |
| `linux-run-by-class.tsv`, `summary.json` | | section 9's tables and every count quoted here |

Object-model spellings are counted as `.Name` text, not resolved receivers (the resolved census
is dc-3zb90.1), so `Open` includes `Workbooks.Open` and `Connection.Open`.

**VBA library, by class** (CalcVBA's classes; projects clean / suspect, top by use):

| Class | Verdict off Windows | Most used |
|---|---|---|
| pure (137 rows) | native | `Len`, `Left`, `Mid`, `Val`, `CLng`, `IsEmpty`, `Array`, `Trim`, `TypeName` |
| locale (29) | model | `CStr` 86 / 281, `InStr` 55 / 199, `Replace` 47 / 281, `Format` 71 / 249, `Split` 20 / 175, `CDbl` 26 / 161, `IsNumeric` 59 / 135, `UCase` 41 / 124 |
| shim (25) | shim or model | `Chr` 78 / 158, `Dir` 22 / 104, `Environ` 3 / 75, `Kill` 13 / 67, `MkDir` 4 / 55, `StrConv` 3 / 47, `Asc` 14 / 55 |
| host (13) | host | `MsgBox` 252 / 339, `Now` 49 / 170, `Timer` 19 / 77, `Unload` 68 / 107, `InputBox` 37 / 72, `Date` 18 / 65 |
| windows (10) | replace or reject | `CreateObject` 29 / 262, `Shell` 0 / 50, `GetObject` 4 / 31, `SendKeys`, `AppActivate`, Declare, `AddressOf` |

**Excel object model, host-reach members** (spellings; projects clean / suspect):

| Need | Members | Linux | web-wasm |
|---|---|---|---|
| internal clipboard | `Copy` 198 / 105, `CutCopyMode` 123 / 57, `PasteSpecial` 103 / 37, `Paste` 100 / 38, `Cut` 28 / 14 | model | model |
| `ui.forms` | `Show` 123 / 191 | replace | replace |
| `path.windows` | `Path` 29 / 187, `FullName` 22 / 54, `PathSeparator` 6 / 30 | shim | shim |
| `fs`, `path.windows` | `Open` 44 / 156, `SaveAs` 45 / 115, `Export` 3 / 42 | shim | mediated |
| `fs` | `Save` 47 / 58 | native | mediated |
| `print` | `PrintOut` 86 / 20 | host | host |
| `ui.filedialog` | `GetOpenFilename` 25 / 32, `FileDialog` 5 / 64, `Dialogs` 21 / 18, `GetSaveAsFilename` 14 / 24 | host | mediated |
| `ui.status` | `StatusBar` 21 / 92 | host | host |
| timer, `block` | `OnTime` 10 / 31, `Wait` 2 / 45 | host / native | host / shim |
| none (dynamic) | `Run` 25 / 90, `VBProject` 3 / 66 | model | model |

Everything else in the top 200 is the document model (`Value`, `Range`, `Select`, `Cells`,
`Font`, `Interior`, `Worksheets`, ...): `native` on every host. Whether a member is *implemented*
is the other question, tracked by the macro-host epic dc-oxh8w.

## 11. Detailed design

### 11.1 Where the checks live

| When | Check | Where | Output |
|---|---|---|---|
| **Load** | Footprint of the project and the formulas against the host profile and the level | CalcVBA `portability::check_project` (exists for the VBA matrix) extended with effects over the call graph; calc-host joins host-member needs from the descriptor and formula needs from the catalogue | the report: per entry point `runs`, `may stop`, `cannot run`, with sites |
| **Compile and bind** | References against the type-library table; early-bound members against descriptors; `PtrSafe` | CalcVBA binder | compile errors only where Excel has them; *types only* libraries bind |
| **Invoke** | must-reach of the entry point against the profile | calc-host-vba command route (`run_procedure`) | refused before start, `NotAvailableHere`, in the invocation record |
| **Call** | `needs` of the member, then `decide` | one place per face: `ObjectModel` policy check (`calc-object-model/src/policy.rs`) for host members; the HAL facet for VBA statements; the class table for activation | Excel's error to VBA; `NotAvailableHere` or `PolicyRefused` to the host; an audit record either way |

One implementation of the availability check, beside the one `decide`, read by all three faces.

### 11.2 Data

- `descriptor/needs.json`: `{ "members": { "_Workbook.SaveAs (Method)": ["fs", "path.windows"] },
  "xlc": {...}, "xlf": {...} }`, loaded like `reach.json`; an unknown member fails the build.
- `HostProfile`: need to provision, one constant per host kind in calc-host, overridable by the
  embedder (a server that mounts a share, a page that is cross-origin isolated). CalcVBA's
  `HalProfileId` and `HostPolicy.allow_*` become views of it (CODE_EXECUTION_POLICY 4.3 already
  says `allow_*` should describe the platform).
- The mount table: host configuration plus session mounts.
- CalcVBA's matrix: `Feature { needs }` instead of `support: [..; 5]`, with the per-host array
  computed and the existing counts kept as a test.

### 11.3 What the person sees

- **A portability badge on a workbook**, beside the macro bar's capability count: *Runs here*,
  *Runs here with N things emulated*, *Some macros cannot run here (N of M)*, *Cannot run here*.
  It opens the report: per macro, what it needs, in the person's terms ("writes a file next to
  this workbook", "starts Outlook: not available on Linux"), and the route when there is one
  ("opens in DNA Calc on Windows").
- **The report CLI.** `dnacalc portability <book> [--host linux|macos|windows|server|web-hosted|web-wasm|wasi|all] [--json]`.
  `--host all` prints the row of badges. It joins CalcVBA's `check --portability` with the
  host-member and formula needs. The corpus form of it replaces this bead's scripts.
- **Running a macro.** A button whose macro is `cannot run` shows the refusal card instead of
  starting. A macro that `may stop` runs; if it reaches the missing thing, the person sees the
  ordinary VBA error with a second line naming the capability and the route.
- **In the web app.** The same badge. A file the macro opens comes from the session's drive, with
  "Add files" (upload, or a folder picker where the browser has one) when a path is absent. A
  save lands in the session drive and offers a download. Dialogs are the page's. Native add-ins
  show `NotAvailableHere` with the route to the desktop, as the skin contract already says.
- **Triage.** The Enable prompt (CODE_EXECUTION_POLICY 2.6) lists what the workbook will ask for.
  The portability lines sit in the same list, marked "not available on this device" rather than
  "will ask".

### 11.4 Beads

Extending dc-8svfs (CalcVBA side) with the DNA Calc side and the measurements. New beads are
related to dc-8svfs:

| Bead | Priority | What |
|---|---|---|
| `dc-x8vql` | P1 | The `needs` overlay beside `reach.json`, `HostProfile`, one availability check beside `decide`, `NotAvailableHere` with Excel's error from the object-model route (1.3, 2.2, 11.1) |
| `dc-0uw7j` | P1 | Path persona in calc-host: mount table, reverse map, `ThisWorkbook.Path`, `DefaultFilePath`, `PathSeparator`, file dialog answers (section 4; the DNA Calc side of dc-8svfs.3) |
| `dc-4hzu2` | P1 | `dnacalc portability <book> --host <h>`: per entry runs, may stop, cannot run; the badge reads it; its corpus form replaces this bead's scripts (9, 11.3; extends dc-8svfs.1) |
| `dc-yfzx1` | P2 | CalcVBA: may and must effects over the bound call graph, constant propagation of ProgIDs, `Run` targets and path prefixes (8.1) |
| `dc-1j8g7` | P2 | CalcVBA faithful mode rejects per call; strict keeps rejection at load (section 3; owner decision 1) |
| `dc-9brxh` | P2 | Type-library table: types-only descriptors, 429 at creation, unknown references refuse only the procedures that name them (5.2, 5.3) |
| `dc-enu66` | P2 | `ui.dialog`, `ui.filedialog`, `clipboard`, `print` join the shared capability names; the skin `Capability` list becomes a view of the host profile (1.2) |
| `dc-xy16l` | P2 | `CELL("filename")` and `INFO` under the persona; needs for host-reach worksheet functions; link targets through the mount table (7.3) |
| `dc-9x1fr` | P2 | C API: needs for `xlc` and `xlf`, `xlfGetWorkspace` and `xlfGetDocument` info types under the persona, XLM file functions through the file facet (7.2) |
| `dc-2r0xe` | P2 | Windows captures: Excel's errors for absent drives, shares, DLLs, entry points, class servers and references; `Dir` order; `GetAttr` of a new file (4.3, 4.4, 5.3, 6.1, 7.3) |
| `dc-smdg7` | P2 | `dnacalc run`: an answers source for `MsgBox` and `InputBox` in the headless CLI (53 entries stop there, 9.2) |
| `dc-ljnou` | P3 | Owner decision: source-compatible XLLs and XLLs compiled to wasm (7.1) |

Existing beads this design gives numbers or scope to: dc-8svfs.1 (the report: add effects over
the call graph, section 8), dc-8svfs.3 (path map: sections 4.3 and 4.4 are its spec),
dc-8svfs.6 (Environ: 4.5), dc-8svfs.7 (Declare table: 6.2 is its order), dc-8svfs.8 (classes: 5.2
is its order), dc-8svfs.14 (browser), dc-a6ee2 (Guarded measurement: section 9's grant column is
its first static estimate), dc-hk7im (formula routes under Guarded), dc-1jia7 and dc-oxh8w.73
(forms).

### 11.5 Owner decisions (2026-11-28)

The owner took all six, and found the plan aligned with the intent:

1. **Reject per call: yes.** Faithful mode refuses at the call; strict mode keeps rejection at
   load (dc-1j8g7).
2. **Unknown references: yes, run what can.** Spurious references are common. But the file
   indicators must list every unknown reference even when nothing uses it yet; otherwise a
   workbook reported "portable" with the reference already in it breaks unexpectedly the first
   time that reference is used. The report and badge show such a workbook as carrying a dormant
   reference, never as plainly portable (dc-9brxh, dc-4hzu2).
3. **XLLs: source compatibility by plain C recompile.** It need not be perfect; we supply
   `#define` headers for the `windows.h` types `xlcall.h` relies on. No magic mechanism: no PE
   loading or emulation; PE binaries off Windows stay `NotAvailableHere`. Wasm XLLs (option C) are
   not planned now. Built after the Linux C API dispatcher (dc-ljnou).
4. **The four capability names: yes**, added now (dc-enu66).
5. **Default mounts as recommended**, and the mount table is configurable host setup: these are
   defaults a user or deployer changes, not fixed rules (dc-0uw7j).
6. **`Shell`: agreed**, reject plus the per-project command map. A careful mechanism may come later.

The recommendations as first put to the owner:

1. **Reject per call in faithful mode** (section 3), changing CalcVBA's "reject at load" to apply
   only in strict mode. Recommended: yes; it is what lets the chart button run.
2. **Unknown references** (5.3): modules that do not use the library still run. Recommended: yes.
3. **Source-compatible and wasm XLLs** (7.1 B and C). Recommended: decide after the Linux C API
   dispatcher exists; prefer C.
4. **Four new Guarded capability names** (1.2): `ui.dialog`, `ui.filedialog`, `clipboard`,
   `print`. Recommended: add them to the shared list now, allowed in Guarded as today, so the
   audit shows them.
5. **Default mounts** (4.3): `C:\Users\{user}` on the home directory, or a private tree only with
   the workbook's folder mounted. Recommended: the private tree plus the workbook's folder under
   Guarded; the home mapping under Unguarded.
6. **`Shell`.** No general shim. Recommended: stay with reject and CalcVBA's per-project command
   map, and never translate command lines.
