# Macro and add-in security modes: names, scope and switches

Status: design note for the owner (dc-oxh8w.7, lane sn1, 2026-11-24). No code. The owner decides the
name of the first mode.

Owner ruling 2026-11-24: no Trust Center and no GPO-based control. The model stays the three levels
that exist (dc-15dk, `crates/calc-macro-security`; dc-8ovd, `docs/architecture/CODE_EXECUTION_POLICY.md`).
This note fixes the name of the first level, what each level covers per runtime, where it is set,
and what the host refuses. It adds no new level and no new store.

## 1. The name of the blocked state

Today the code and the docs call it **Off**. That is the weak point: "Off" reads as "the macro
feature is switched off", yet the level is a hold the person lifts per file with Enable (and Unblock
and enable for a marked or network file). Candidates:

| Name | For | Against |
|---|---|---|
| **Held** | Matches what happens: the project is held, the bar says "held until you enable it". Already the word in the macro-security crate docs ("held on open"). Short, distinct from Excel's "Disable all", and not a verb for the owner's banned process words. Sorts naturally beside Guarded and Unguarded as a state, not an action. | New to Excel users; needs one line of help text. |
| **Disabled** | Excel's own word ("Disable all macros with notification"); no explanation needed. | Suggests the code cannot be turned on; it hides the per-file Enable. Excel has four variants of it, so people expect a Trust Center behind it. |
| **Locked** | Short, plain. | Collides with sheet and workbook protection ("locked cells", "locked project"); VBA projects can be "locked for viewing". |

**Recommendation: Held.** The three levels then read Held, Guarded, Unguarded, in order of how much
the code may do. `--macros held|guarded|unguarded` is the flag; `off` stays accepted as an alias
(like `safe` for `guarded`), and the stored `macro-security.txt` value `off` is still read.

## 2. What each level means

One level per application, applying to everything a workbook or add-in brings. Capability names are
those of CODE_EXECUTION_POLICY.md section 2.2 (`fs.read`, `fs.write`, `process`, `env`, `settings`,
`ffi`, `com.activate`, `ui.keys`, `network`, `project.write`).

| Runtime | Held | Guarded | Unguarded |
|---|---|---|---|
| **VBA** (workbook project, `.xlam`, loaded modules) | Project not compiled or run. The workbook opens and calculates; its UDFs and events do not exist. | Runs through CalcVBA's hosting interface. Origins (a) host objects and (b) host classes are available; (c) vouched OS classes only as pure or projected; (d) everything else is refused with error 429. Each member's reach is checked against the policy. | Runs with the person's rights, through the same interface with an allow-everything policy. Still audited. |
| **XLL** (native, in process or in the add-in surrogate) | Not loaded. A workbook that calls its functions gets `#NAME?`, as in Excel with the add-in off. | **Not loaded.** Native code cannot be mediated, so Guarded never loads it. The refusal is typed (`NativeCodeNeedsUnguarded`) and shown in the Add-in Manager. | Loaded. `xlAutoOpen` runs. |
| **COM add-ins** (automation, COM-visible `.xla*`, shared add-ins) | Not loaded; `COMAddIns` lists the entry as not connected. | **Not loaded**, same reason as XLL. VBA's own `CreateObject` of a COM class follows the VBA column, not this one. | Loaded; `OnConnection` runs. |
| **JavaScript add-ins** (custom functions, task panes) | No module load; no pane. | Custom functions run in the embedded QuickJS with time and memory limits, inputs only worksheet values and invocation metadata. Task panes run in their host sandbox (WebView2 or the browser); their only bridges out are the host's Office.js projection and fetch, both mediated by the policy (`network` to the manifest's declared origins only). | The same sandboxes. The host bridges allow everything the manifest declares. A pane never gets more than its sandbox lets a web page have. |

Consequences worth stating plainly:

- Guarded is where VBA and JavaScript can run without trust in the file. It is not a fence for native
  code, and the product never says it is.
- Unguarded is the only level that runs native code. That is Excel's "Enable" and nothing more.
- A level never reaches back into the file: a workbook cannot declare itself trusted, carry a
  setting, or change the level (`project.write` is refused in Guarded; no member writes the level).

## 3. Where the person switches it

Per application (the default for every file the app opens):

- **Settings, Security page** in DNA Calc, and the same control in the Formula Explorer's settings
  (DNA OneCalc). Three radio buttons with one sentence each, and the active level always visible in
  the status area (dc-15dk). Stored per user, `macro-security.txt`, as today; the browser build
  keeps it for the session only (dc-15dk remainder).
- **Command line and environment**, for scripts, agents and the corpus harness only: `--macros
  held|guarded|unguarded`, `--trust-macros`, `DNACALC_TRUST_ALL_MACROS=1`. A pinned level is shown
  with what pins it, is never stored, and the page cannot change it. The harness never executes a
  corpus file outside its manifest (`ProjectGrant::evidence_manifest`).

Per workbook (a one-file exception, set by the person while the file is open):

- **The macro bar**, shown when a file brings code under Held. Buttons: **Enable guarded**, **Enable
  unguarded** (offered only when the file has native or privileged content, or the person asks), and
  for a marked or network file **Unblock and enable** behind the short warning. The choice applies
  to this file in this session and, if the person ticks "remember for this file", is stored in the
  per-user store keyed by the file's content hash (not its path, so a changed file asks again).
- A per-workbook choice may be above the app level (Held app, one file Guarded) or below it
  (Unguarded app, one file Held through "Hold macros for this file"). It never outranks a pinned run.
- Add-ins are per add-in, in the **Add-in Manager**: each row shows its level and a switch. An XLL or
  COM add-in has only Held and Unguarded; a JavaScript add-in has all three.

There is no per-folder or per-publisher list (no Trusted Locations) and no signing exemption in this
model. Signing stays an open item of dc-8ovd; if added, it may only change which prompt is shown,
never lift a level silently.

## 4. What the host refuses, per level

| Request | Held | Guarded | Unguarded |
|---|---|---|---|
| Run a VBA project, event or UDF | refused | allowed | allowed |
| `Shell`, `AppActivate`, start a program (`process`) | n/a | refused | allowed, audited |
| `Open`, `Kill`, `FileCopy`, `Dir` outside the host's file facet (`fs.read`, `fs.write`) | n/a | refused (scoped host files only) | allowed |
| `Environ`, `GetSetting`, `SaveSetting` (`env`, `settings`) | n/a | refused | allowed |
| `Declare`, `AddressOf` to native code (`ffi`) | n/a | refused, except entries the engine implements | allowed |
| `CreateObject`, `GetObject`, `New` of an outside class (`com.activate`) | n/a | origins (a)-(c) only; (d) gets 429 | allowed |
| `SendKeys` (`ui.keys`) | n/a | refused | allowed |
| `Workbooks.Open` of a URL, `FollowHyperlink`, XMLHTTP (`network`) | n/a | refused until a projected class exists | allowed |
| `VBProject`, `VBComponents.Import` (`project.write`) | n/a | refused | allowed |
| `RegisterXLL`, `Application.Run` of an unloaded add-in | refused | refused | allowed |
| Load an XLL or COM add-in | refused | refused | allowed |
| JavaScript custom function or pane | refused | sandbox, mediated bridges | sandbox, open bridges |
| Change the level from inside code or a workbook | refused | refused | refused |

Every request that reaches the policy is audited at every level, Unguarded included (sequence,
capability, origin, subject, decision, VBA error number). The macro bar shows a per-workbook count
and the list behind it (CODE_EXECUTION_POLICY.md section 2.5).

A refusal is typed and visible: VBA gets error 429 or 70 as Excel would for a denied operation, a
native add-in gets a named refusal in the Add-in Manager, and a JavaScript bridge call rejects with a
policy error. Nothing is skipped silently, and no refused request counts as a pass in any corpus
statistic.

## 5. What this changes in the repository

1. Rename `MacroSecurity::Off` to `Held` (key `held`, `off` read as alias), the settings text, the
   bar copy and `docs/user/formula-explorer.md`; owner confirms the name first.
2. Per-file "remember" store and the "Hold macros for this file" button (app level above file level).
3. Add-in Manager level switch for XLL, COM and JavaScript add-ins; the typed
   `NativeCodeNeedsUnguarded` refusal. These belong under dc-8ovd and the add-in beads
   (dc-17spk for JavaScript); none is built by this note.

## 6. Open for the owner

1. The name: Held (recommended), Disabled, or Locked.
2. Whether "remember for this file" (hash-keyed) is wanted at all, or every session asks again.
3. Whether Guarded may offer a per-add-in "load unguarded" without moving the whole app level
   (assumed yes above, since an XLL needs it).
