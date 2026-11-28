# Code execution policy

Status: draft for the owner (dc-8ovd, lane hd1, 2026-11-20). Nothing here is built yet beyond what
section 4.1 lists as today's state. It is based on the seam audit of the same day (lane sa1,
CalcVBA `66777706`, DnaCalc-engine `518c8f18`; finding numbers F1-F16 refer to it). It is written
against the end state of CalcVBA's VARIANT migration (lane vx1, dc-yqq0): one platform VARIANT
cell across the seam, `ComValue` gone, the host API settled.

One policy covers every runtime that runs code a workbook or add-in brings: VBA (through CalcVBA),
native XLL and COM add-ins, and JavaScript add-ins later (dc-1nbp.30). The details differ per
runtime. The levels, the capability names, the audit and the settings are shared.

## 1. The levels

The person chooses one level per application (crates/calc-macro-security; dc-15dk). A run can be
pinned with `--macros off|guarded|unguarded`, `--trust-macros` or `DNACALC_TRUST_ALL_MACROS=1`.

| Level | Meaning |
|---|---|
| **Off** | Code is held. The workbook opens and calculates without it; the bar offers Enable. Excel's default. |
| **Guarded** | Code runs. Everything it can reach outside the calculation (files, network, processes, registry, other COM servers, native code) is reached only through the host, which decides, mediates and records each use. Was "Safe". |
| **Unguarded** | Code runs with the person's full rights, as Excel's "Enable" does. Still audited. |

What each level means per runtime:

| Runtime | Off | Guarded | Unguarded |
|---|---|---|---|
| VBA (CalcVBA) | not compiled to run | runs; the CalcVBA hosting interface (section 4) mediates every outside reach | runs; the same interface, with a policy that allows everything |
| XLL and COM add-ins (native code) | not loaded | **not available.** Native code in our process (or in the surrogate, ADDIN_SURROGATE.md section 9) has the person's full rights and cannot be mediated. Guarded never loads native code. | loaded |
| Embedded JavaScript custom functions | no module load | runs with time and memory limits in QuickJS, with worksheet values and invocation metadata as inputs | same embedded runtime capabilities |
| JavaScript task panes (dc-17spk.5) | not loaded | runs in its host's sandbox (WebView2 or the browser); its only bridges out are the host's Office.js projection and the host's fetch, both mediated by this policy | the same sandbox; the host bridges allow everything |

The rest of this section describes the closely related VBA policy and object model.

## 2. Guarded is a model, not a deny list

Today's Guarded ("Safe", dc-10un) is a list of refused capabilities. It broke on the first
real case (dc-g4mj3): DNA Calc's own `Application`, `ActiveSheet` and `Selection` were refused,
because fetching the host's root object went through the same check as activating an outside COM
class, and the two could only be told apart by a ProgID list and an op-name string.

The fix is to answer two separate questions for every object VBA touches:

1. **Where does it come from?** (its *origin*, decided once, when the object is obtained)
2. **What can it reach?** (its *reach*: which capabilities its members use)

An origin being allowed never implies that its reach is allowed.

### 2.1 Origins

| Origin | Examples | In Guarded |
|---|---|---|
| **(a) Injected by the host** | `Application`, `ActiveWorkbook`, `ActiveSheet`, `Selection`, `ThisWorkbook`, `Sheet1` and the other document objects, `WorksheetFunction` | Always available. Getting one is never an activation and is not a policy question. |
| **(b) Offered by the host** | Classes the host implements and offers to `CreateObject` and `New`: the engine's `Scripting.Dictionary`, `VBScript.RegExp`, a host class such as a DNA Calc chart builder | Available. The host wrote the code, so its reach is what the host's own members declare (2.2). |
| **(c) Vouched for by the host** | Outside COM classes the host names by CLSID: the OS `Scripting.Dictionary` (scrrun.dll), `VBScript.RegExp` (vbscript.dll), `MSXML2.DOMDocument.6.0` | Available only if the vouched entry says what the class reaches, and that reach is allowed or projected (2.3). |
| **(d) Everything else** | Any other registered class, any running object, any moniker | Refused (error 429, audited). |

Objects obtained *from* an object VBA already holds (a `Range` from `ActiveSheet`, a `Match` from
a `RegExp`) take the origin of the object that returned them. Calling members on, querying
interfaces of, or enumerating an object VBA already holds is not checked again: it was checked
when the object was obtained. Its members' reach still is (2.2).

CalcVBA's built-in library (`Collection`, `Err`, `Format`, the VBA functions) is the engine's
own and needs no origin; its functions that reach outside (`Shell`, `Kill`, `Open`, `Environ`,
`SaveSetting`, `SendKeys`) are capabilities like any other.

### 2.2 Reach and the shared capability names

One vocabulary across the runtimes, used by triage, the audit and the settings. The first eight
exist in CalcVBA's HAL capability table.

| Capability | Covers |
|---|---|
| `fs.read`, `fs.write` | `Open`, `Dir`, `FileCopy`, `Kill`, `MkDir`; file-reading members of any object |
| `process` | `Shell`, `AppActivate`, starting another program |
| `env` | `Environ` |
| `settings` | `GetSetting`, `SaveSetting` and other registry access |
| `ffi` | `Declare`, and `AddressOf` passed to native code |
| `com.activate` | Creating or binding an origin (c) or (d) object |
| `ui.keys` | `SendKeys` |
| `network` *(new)* | HTTP, sockets, URLs opened by any member (XMLHTTP, WinHttp, `Workbooks.Open "https://..."`, `FollowHyperlink`) |
| `project.write` *(new)* | Changing VBA code or references (`VBProject`, `VBComponents.Import`), in this or another workbook |
| `ui.dialog` | Message and input dialogs |
| `ui.filedialog` | File and folder pickers |
| `clipboard` | Access to the system clipboard |
| `print` | Print and print preview |

The object-model policy audits the four UI capabilities and allows them in Guarded where the
host supplies them.

Reach is declared per member, not per class, wherever the member is the host's:

- **The host's own object model has reach.** `Workbooks.Open`, `SaveAs`, `FollowHyperlink`,
  `ExecuteExcel4Macro`, `RegisterXLL`, `Application.Run` of something not loaded, `VBE` all reach
  outside. Origin (a) does not make them free. Each member's reach is written once, in
  calc-object-model's descriptor (descriptor/excel.json), beside its DISPID, and both the VBA
  route and the COM projection read it from there. A member with no reach entry is treated as
  reaching nothing only if it is pure calculation (most of the model); the descriptor review
  marks the rest.
- **An offered class (b)** declares its members' reach the same way, in its own descriptor.
- **A vouched class (c)** cannot be inspected: its code is native and opaque. Its entry declares
  its reach for the whole class.

### 2.3 Mediation: allowing a class never allows its capability

A vouched class is in one of three states:

| State | Meaning | Examples |
|---|---|---|
| **Pure** | It reaches nothing but memory. Activating it is audited as `com.activate` and allowed. | OS `Scripting.Dictionary`, OS `VBScript.RegExp` |
| **Projected** | It reaches the OS, so VBA never gets the OS class. It gets an engine or host class (origin (b)) with the same CLSID, interfaces and type library, whose outside reach goes through the host's file, network or process facets, and therefore through this policy. | `Scripting.FileSystemObject` over the host's file facet; `MSXML2.XMLHTTP` over its network facet (both later) |
| **Refused** | It reaches the OS and no projection exists. 429 in Guarded. | `WScript.Shell`, `ADODB.Connection`, `ADODB.Stream`, `Shell.Application`, `MSXML2.DOMDocument` until projected (its `load`, `save` and external entities read files and URLs) |

So projection is not "the OS class with a filter in front": we cannot filter opaque native
code member by member. It is a replacement class. A class with outside reach that has no
replacement is refused, however common it is.

Two rules protect a vouched entry:

- It names a CLSID, not a ProgID. The ProgID is translated once (section 4.3), and the
  server it resolves to must be the machine-wide registration (HKLM) of a file under the
  system directory. A per-user `HKCU\Software\Classes\CLSID` override, `TreatAs` or
  `AutoConvertTo` redirect turns it into origin (d).
- Off Windows (Linux, wasm32) there is no OS COM: origin (c) is empty and the engine's
  classes (b) serve.

### 2.4 Other routes out

| Route | Rule in Guarded |
|---|---|
| `GetObject(path)` and other monikers | Binding a file moniker reads the file and activates its server: `fs.read` plus `com.activate` of origin (d). Refused. |
| `GetObject(, "Class")` (running objects) | Reaches another process. Refused, unless the host maps the class to one of its injected objects (open question 7). |
| `Declare` | `ffi`. Refused, except for a short list of entries the engine implements itself and the host allows (open question 5). |
| `AddressOf` | Only reaches native code through `Declare`, so it follows `ffi`. Inside the engine (`CallByName`, engine callbacks) it is not a capability. |
| Events (`WithEvents`, document events) | No new check. The source is an object VBA already holds legitimately; the handler is the project's own code under the same level. |
| Type-library metadata | Reading the metadata of a declared, registered reference is not an activation (F3). Allowed and audited. |
| Add-ins (`.xlam`) and modules a person loads | Run under an explicit level, chosen like a workbook's (open question 6). A load path without a level does not run the code (today several run with no policy at all, F7). |
| `project.write` | Refused. Code that changes code would otherwise persist into files. |
| Native add-ins called from VBA | Calling a function of an XLL already loaded under its own grant is like a cell calling it: allowed. Loading native code (`RegisterXLL`, `Application.Run` of an unloaded add-in) is refused. |

### 2.5 The audit

Every request that reaches the policy is recorded, allowed or denied, at every level including
Unguarded: sequence number, capability, origin, a typed subject (path, URL, command, CLSID,
`lib!symbol`, variable or setting name), the decision and the VBA error number. The host owns
where records go (section 4.2). The applications show a per-workbook count ("this workbook
asked for files 3 times, refused") in the macro bar, with the list behind it. The corpus
runner drains it per case, as today (dc-10un).

### 2.6 Choosing the level (triage)

CalcVBA's static triage predicts a project's capabilities from bound code (dc-10un step 2). The
applications use it to tell the person what a workbook will ask for before Enable. Corpus and
test policy, from dc-8ovd:

- Own code and fixtures: Unguarded.
- Public corpora: Guarded.
- Web-harvested files: one notch stricter: Off unless triage finds nothing suspect, then
  Guarded. Unguarded only after a person reviews and approves the file; the approval is
  recorded with the file's hash.
- Native add-ins from harvested sources: only when built from source.

Signing (Excel's trusted publishers) is open (question 10).

## 3. What changes from today's Guarded

| Today ("Safe") | Guarded |
|---|---|
| Host root objects refused unless an op string and a ProgID list match | Origin (a): never a policy question |
| `com.activate` refused for every class | Origins (b) and pure (c) allowed; projected (c) replaced; the rest refused |
| Native COM policy scope refuses invoke, QI and enumeration on objects already held (F2) | Checked once, when the object is obtained |
| The host's object model members not checked at all | Each member's reach from the descriptor |
| Standard type libraries allowed by a GUID list in DNA Calc (F3) | Metadata of declared references is not an activation |
| No `network` or `project.write` | Both named, refused in Guarded |
| Several load paths run unwrapped (F7) | No level, no run |

## 4. The CalcVBA hosting interface

### 4.1 Today (sa1 audit, CalcVBA 66777706)

| Piece | What it does | Problem |
|---|---|---|
| `HostServices::is_host_class`, `authorize_com_activation(op: &str, target: &str)` (calcvba-hal traits.rs:107-124) | "Is this the host's?" by ProgID; the kind of request by op name | F1, F9: strings, and asked before the host is |
| `PolicyHost` (capability.rs:405-583) | Safe/Full wrapper over any `HostServices`, 8 capabilities, bounded audit | Good base; exempts host classes only for `"create_object"` (dc-g4mj3) |
| `HostPolicy.allow_*` (model.rs:143-155) | A second set of allow flags the standard adapter reads | F2: a second decision point |
| `ComPolicyScope` (calcvba-com access_policy.rs) | Thread-scoped check at ~20 native sites by op string | F2: a third point; refuses calls on held objects |
| `permits_com_activation` | Changes binding (engine vs OS Dictionary) at compile time | F2, F4: a fourth input |
| `ComHal::app_object_variant`, `register_app_object`, `document_object_variant`, `HOST_DOCUMENT_FIELD` | Two ways for host objects to reach VBA | F11 |
| `ProjectReference::HostInjected{"Lib.Coclass"}` | One string as library name, coclass, ProgID and audit target | F9 |
| Engine classes picked by ProgID in lib/host.rs:618, engine.rs:641 | RegExp before policy, Dictionary at bind time | F4 |

On the DNA Calc side the same knowledge is spread over `SpreadsheetHost::is_host_class` and
`create_object_variant` string matches (spreadsheet.rs:837-846, :1208-1240), the GUID list in
calc-vba policy.rs:70-106, and two copies of the level-to-mode mapping (F6).

### 4.2 Target: what a host supplies

One configuration, handed to CalcVBA when a session is created, plus facets it calls at run time.
Data where possible, so triage and binding can use it before anything runs.

| Part | Kind | Content |
|---|---|---|
| **Injected objects** | data + one facet | A list of host names, each with its type (a coclass in a type library the host's resolver supplies) and its role (application root with global members, document object for a module, named global). The facet `host_object(name) -> VARIANT` returns the instance. Names are opaque to CalcVBA. This replaces `app_object_variant`, `document_object_variant`, `HOST_DOCUMENT_FIELD` writing and `is_host_class`. |
| **Offered classes** | data + one facet | Each class: CLSID, any ProgIDs it answers to, its type library, its members' reach. The facet `create(clsid) -> VARIANT`. The engine's own Dictionary and RegExp are offered the same way, by CalcVBA's default configuration, so a host can drop or replace them. |
| **Vouched classes** | data | Each entry: CLSID, state (pure, projected by which offered class, refused). |
| **Policy** | one trait | `decide(request) -> allow / deny(error)`. The request is typed: capability, origin, operation (an enum, not a string), subject. CalcVBA ships the standard implementations, one per level; a host may wrap one (for example to allow reads under the workbook's folder). |
| **Mediated facets** | the existing HAL facets | `FileSystemHal`, `ProcessEnvHal`, the settings and dynlink facets, and a new network facet. In Guarded these are the host's mediated implementations; projected classes use them too. |
| **Audit sink** | one trait | `record(audit record)`. CalcVBA's bounded `AuditHandle` becomes the default sink. |

### 4.3 What CalcVBA decides itself, and what it asks

CalcVBA decides, with no host names in its logic:

- **Class resolution, in one step.** For every `New`, `CreateObject`, `GetObject` and
  host-injected reference it produces a typed origin: host-injected (by name), offered (by CLSID),
  vouched (by CLSID), other (by CLSID), moniker, or running object. A ProgID string is translated
  to a CLSID exactly here: first through the host's offered table, then through the platform
  registry. After this step nothing compares strings. Binding uses the same result, so the
  compile-time choice of Dictionary follows the configuration, not a separate flag.
- Which capability each library function, `Declare`, statement and moniker needs (the one
  table triage already uses), and the VBA error number for each refusal (70, 75, 53, 429).
- That calls on held objects, events and type-library metadata are not activations.
- When to call the policy: once per obtained object of origin (c) to (d), once per capability use.

CalcVBA asks the host:

- for injected objects and offered instances;
- for every policy decision, through `decide`;
- to perform every outside reach in Guarded, through the mediated facets;
- for member reach of host objects: from the type information the host supplies (the
  descriptor's reach becomes a custom attribute or a side table keyed by DISPID; open question 8).

There is **one decision point**: `decide`. `HostPolicy.allow_*` becomes a description of what the
platform adapter can do at all (no process spawning on wasm32), not a policy. `ComPolicyScope`
keeps only activation and type-library loading, and calls `decide`.

### 4.4 How each host configures it

| Host | Injected | Offered | Vouched | Mediation and audit | Levels |
|---|---|---|---|---|---|
| **DNA Calc** (desktop, worker) | Application, ActiveWorkbook, ActiveSheet, ActiveWindow, Selection, ThisWorkbook and document modules, all from the calc-object-model descriptor | Engine Dictionary and RegExp; host classes later | Windows: OS Dictionary and RegExp as pure; FileSystemObject projected once its replacement exists; the rest refused | File facet scoped by the host; network through calc-platform-desktop's WinHTTP once allowed; per-workbook audit in the macro bar | Off, Guarded, Unguarded |
| **DNA Calc - Formula Explorer** (values only, desktop and browser) | Application with the values-only profile (WorksheetFunction, Caller, Volatile); no workbook objects | Engine Dictionary and RegExp | None (the browser has no COM; keep desktop the same so behaviour matches) | All outside facets refuse in Guarded; audit in the formula pane | Off, Guarded, Unguarded |
| **CalcDb** (planned; its shape is assumed here) | Its own object model, from its descriptor | Engine classes | None | Server log as the audit sink; no UI | Guarded only (question 11) |
| **CalcVBA inside Excel** (tools/calcvba-excel) | Excel's real Application through `calcvba_set_app_object` | Engine classes | The loader's list | Excel's own members reach the OS natively; Guarded needs a DISPID reach table for them (question 9) | Excel's own macro security has already let the workbook run; the loader chooses Guarded or Unguarded |
| **calcvba-cli** | None, or a test host's | Engine classes | None by default; `--vouch` for tests | Audit printed as JSON | `--policy guarded|unguarded` (`safe` and `full` accepted as old names) |

### 4.5 What changes, in which order

**DNA Calc side, now** (independent of vx1; DNA Calc owns all of it):

1. dc-g4mj3 stopgap, as sa1 proposed: CalcVBA exempts `app_object` for host classes
   (capability.rs:577, host-neutral, one line and a test), and DNA Calc's `SpreadsheetHost` gains
   `app_object_variant`. This keeps today's string matching for one pin; it is removed in step 8.
2. Safe to Guarded in DNA Calc's names and settings (lane gm1), with `safe` still parsed.
3. F7 and F6: every VBA load path (worker, OneCalc, calc-addin-host xlam, calc-host-vba without a
   grant) takes an explicit level; no level means the code does not run. One level-to-mode mapping.
4. One table in calc-vba of DNA Calc's injected names, offered classes and vouched CLSIDs (empty
   for now), generated from the descriptor where it can be. Today's `is_host_class` and
   `create_object_variant` read it instead of matching strings inline.
5. A `reach` field per member in calc-object-model's descriptor; the first review marks the
   members listed in 2.2. `SpreadsheetHost` and calc-com-host refuse reaching members in Guarded
   through one policy call and record them.
6. Show the audit per workbook in both applications.

**CalcVBA side, after vx1** (vx1 is CalcVBA's only priority until done; these touch `ComHal`):

7. The session configuration of 4.2 and the class-resolution step of 4.3, with a typed origin.
8. The injected-objects facet and a `HostObject(name)` native in the binder; delete
   `is_host_class`, the op-name branch and `AppObject` from triage (F1, F9, F11).
9. `decide(request)` with typed requests as the one decision point; `PolicyHost` becomes its
   standard implementation; `HostPolicy` becomes a capability description; `ComPolicyScope`
   narrowed; metadata of declared references exempt (F2, F3). DNA Calc then deletes its GUID list.
10. The `network` and `project.write` capabilities, the network facet, the audit sink trait.
11. `PolicyMode::Safe`/`Full` renamed to `Guarded`/`Unguarded`.
12. Later, as separate beads: projected FileSystemObject and XMLHTTP; the `Declare` list.

**Then** DNA Calc pins that CalcVBA, moves `SpreadsheetHost` onto the new facets and removes the
stopgaps of steps 1 and 4.

## 5. Open questions for the owner

1. **Does Guarded activate any OS class, or only engine replacements?** OS Dictionary and RegExp
   match Excel exactly; the engine's versions may not yet. *Recommendation:* allow them as pure
   vouched classes on Windows; the engine's versions elsewhere and when the host offers no COM.
2. **FileSystemObject and files in Guarded.** *Recommendation:* refuse all file access for now (as
   today). Then add a projected FileSystemObject over a file facet that allows reading under the
   workbook's own folder, after the owner decides how the person sees and widens that scope.
3. **Network in Guarded.** *Recommendation:* name the capability now so triage and the audit show
   it; refuse it in Guarded until a per-workbook allow-list exists.
4. **MSXML.** It is common in harvested VBA, but `load`, `save` and external entities reach files
   and URLs. *Recommendation:* refused in Guarded until an engine DOM exists; `loadXML` of a
   string alone is not enough reason to vouch for the class.
5. **A short `Declare` list.** `Sleep`, `GetTickCount`, `QueryPerformanceCounter` and
   `QueryPerformanceFrequency` are common and reach nothing. *Recommendation:* the engine
   implements these few itself, audited as allowed; anything touching memory (`CopyMemory`) stays
   refused, because it can break out of any containment.
6. **Add-ins and loaded modules.** *Recommendation:* an installed add-in runs at the level the
   person chose when installing it (stored with the add-in's hash), otherwise the application's
   level. Never unwrapped.
7. **`GetObject(, "Excel.Application")` inside DNA Calc.** Should it return DNA Calc's own
   Application? *Recommendation:* measure what Excel returns from its own VBA first; if it is
   itself, the host maps that class to its injected Application (origin (a)).
8. **Where member reach lives.** *Recommendation:* in the calc-object-model descriptor, the one
   source for both projections; CalcVBA receives it as a side table keyed by DISPID with the
   host's type information, not by member name.
9. **Guarded inside Excel.** Excel's own members are native and reach the OS. *Recommendation:*
   the loader offers Guarded only with a DISPID reach table for Excel's objects (the same
   descriptor), refusing members it does not list; until that exists it offers Unguarded only,
   labelled as such.
10. **Signing and trusted publishers.** *Recommendation:* defer; the levels and the audit come
    first, and approval by file hash covers the corpus.
11. **CalcDb.** Its host shape is not written down. *Recommendation:* Guarded only, no vouched
    classes, audit to its log; revisit when its design exists.
12. **Native code under an OS-restricted surrogate** (an AppContainer or a Linux sandbox).
    *Recommendation:* not now; ADDIN_SURROGATE.md section 9 already says the surrogate is not a
    sandbox, and Guarded never loads native code.
13. **JavaScript add-ins.** *Recommendation:* the same three levels; Guarded means the host's
    Office.js projection and fetch bridge use this policy and audit. Decide the rest in
    dc-1nbp.30.
