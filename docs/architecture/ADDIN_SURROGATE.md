# Add-in surrogate: the process boundary for native add-ins

Design note for dc-6332.15 (2026-11-16). It turns answer 13 section B.8 and answer 18 section 3.7
(docs/architecture/review-2026-11/pro/) into our own decisions, with the harvest findings
(harvest/13-harvest.md N4, harvest/18-harvest.md findings 3-5). There is no code in this note. The
implementation is answer 18's Stage 3 (OneCalc) and Stage 4 (DNA Calc). Each gets its own bead once
this note is accepted. Two beads use it now: dc-snz.21 (xlOil safe teardown and reinitialization)
and dc-snz.19 (Excel-DNA Native AOT removal).

## 1. Why

Today both desktop applications run everything in one process. One thread runs the WebView2
page, the XLMAIN/XLDESK/EXCEL7 window chain, the COM single-threaded apartment (STA), the add-in
host (`calc-addin-host`), the XLL main lane and VBA. In DNA Calc the same thread also runs the
calculation session. `$` functions run on a pool of one to four workers (`calc-xll-host/src/worker.rs`).
Native async calls wait under pending keys (`callbacks.rs`). The consequences:

- A native call that hangs freezes the window, as it does in Excel. The person has no way out except
  killing the application.
- A native access violation or a fail-fast ends the application, and with it the only copy of
  any edits not yet saved.
- Unload is unsafe in general. `physical_returned` is set inside `xlAsyncReturn`, before control
  returns to the add-in's own thread, so `close`/`unload_module` can reach `FreeLibrary` while
  module code still runs (harvest 18, finding 4).

Excel has the same exposure. Surviving it is a deliberate product difference: an Excel crash stays
recorded as a crash in every comparison, and a surrogate never turns a crash into a pass.

## 2. The unit is the session, not the DLL

**Decision.** There is one compatibility process (the *surrogate*) per session. Add-ins are not
split one process per DLL. A session's surrogate holds every add-in the session loads, each
add-in's runtime group (an xlOil core with its Python and plugins; an Excel-DNA XLL with its payload
and generated COM/RTD classes), and whatever those add-ins call synchronously:

- **OneCalc:** the `Session` (dna-onecalc) and its `AddinHost` run in the surrogate. The page and
  the window stay in the controller (the process the person started). OneCalc stays values-only.
  Nothing in this design adds a workbook, a caller address or a reference to OneCalc.
- **DNA Calc:** the one active calculation session runs in the surrogate: `calc-host`, the engine,
  the authored state, VBA, the ordinary and `#` XLL lane, the `$` pool, the COM/RTD STA and the
  compatibility windows. The controller keeps the window, the page and the durable copy of the
  document (section 7).

**Why this cut.** XLL callbacks read references, ask for the caller, call worksheet functions
and nest `xlUDF` calls into other add-ins. VBA and native code share the owner thread. If the
session stays with its add-ins, none of that crosses a process boundary and no cached read turns
into IPC. What crosses is the traffic that already crosses a seam today:

| Application | Existing seam | What it carries |
|---|---|---|
| OneCalc | `dna-onecalc/src/port.rs` `Request`/`Reply` and the `Link` trait (serde types, already used between page and session) | formula text, values as text and kinds, add-in lists, file bytes |
| DNA Calc | `dna-calc-model` `WorkbookTransport` and the `wire` `Request`/`Reply`/`Event` messages (already used between page and workbook worker) | commands, projections, events, file bytes |

The surrogate transport is a third implementation of those seams, beside the in-process and
browser-worker ones. Commands and projections are not designed a second time.

**In-process conformance mode stays.** The same composition runs in one process with the local
transport. Every XLL comparison (X1-X5 traces, `xll-accept`, the D-series batches) runs in this
mode, and a surrogate run of the same cases must give the same values, callback codes and free
pairs. One implementation, two transports: there is no second host.

## 3. What stays in the surrogate

The window, the shim and the export have to be in the process the add-ins run in. Excel-DNA
enumerates the main thread's `XLMAIN` windows and their `EXCEL7` children, and xlOil goes from
`xlGetHwnd` to `XLDESK` to `EXCEL7` (XLL_COM_HOSTING.md, "COM pieces the frameworks use"). A
visible window in the controller satisfies neither.

| Concern | Stays in the surrogate | Never crosses |
|---|---|---|
| Native ABI | operands, libffi entry, export snapshots, the callback allocation registry, `xlAutoFree12` and FreeOnly frames | pointers, handles, arena addresses |
| Process identity | the same desktop executable in a surrogate role (`--surrogate`), with its main-image `MdCallBack12` export, version resource (16.x), `EXCELDNA_ASSUME_EXCEL_PROCESS` in its environment, and companion `XLCALL32.DLL` bound by full path | |
| Windows | the hidden XLMAIN/XLDESK/EXCEL7 chain answering `WM_GETOBJECT`, and the STA message pump | HWNDs (the controller's visible window is not a parent of add-in windows) |
| COM | the `calc-com-host` projection, its object table and apartment lifetimes | `IDispatch`/`IUnknown` pointers |
| RTD | server objects, `IRTDUpdateEvent`, the RTD client, topic leases | server pointers |
| Workers | the `$` pool and its owner-serviced callback requests | worker thread identity |

Running the same executable keeps the process name and the version resource identical. The
X-series traces already check both.

## 4. What crosses, and the message envelope

Every message the surrogate transport carries has an envelope around the existing seam payload:

- `incarnation`: 128 random bits chosen when the surrogate starts, plus a start counter the
  controller keeps. It is never the OS process ID, because a restarted surrogate can get the same
  PID.
- `request`: the controller's request ID; a reply names the request it answers.
- `parent`: for a request the surrogate makes of the controller while a controller request is
  open (section 5), the ID of that open request.
- `sequence`: per direction, strictly increasing; a gap or repeat ends the connection.
- `kind`: the payload type, checked against what the request expects before decoding.

Values cross only in owned form: type, binary64 bits, UTF-16 units of text, errors, array shape,
and the difference between missing and empty. This is what `CalcValue` and the seam payloads
already keep. Decoding checks header, size, incarnation and kind before it reads the payload, and
a message from an old incarnation fails before it can change any state. Validation cannot protect
against memory corruption inside the sending surrogate. Containment of a corrupt surrogate means
ending it (section 6).

## 5. Callbacks, reentry and caller identity

With the session and its add-ins in one process, XLL callbacks, nested `xlUDF`, COM calls from
add-ins and VBA all stay local. They keep today's frames, permissions and status codes (the
dc-6332.10 `CallbackReply`, FreeOnly frames, measured denials). What can still cross while an add-in
call is running are requests for services only the controller has:

- dialogs and messages (`xlcAlert`, `MsgBox`, `InputBox`), file pickers, the clipboard;
- later, ribbon, task pane and menu commands, each one a named capability with its own bridge.

Such a request carries `parent` = the open controller request, so the controller answers it
while that request waits. A single blocking request/reply cannot serve a dialog during a UDF. The
controller's wait is a constrained pump. It serves only requests whose parent it is waiting on,
and it holds back unrelated commands, structural edits and unload until the open request
finishes (answer 13 B.7, the D.21 witness). Inside the surrogate the owner's wait for a worker
stays as it is: it serves that worker's callbacks and nothing else.

**Caller descriptors** never cross in Stages 3-4, because callers are resolved where the session
is. If a later runtime-group split needs it, a caller crosses as a tagged logical descriptor
(sheet identity, rectangle, structure generation, caller kind) and is turned back into a local
operand. A made-up A1 caller is never used: it changes results and RTD topic identities.

## 6. Failure: timeout, hang, crash

- **A timeout cancels admission; it does not stop native code.** When a call takes longer than
  its limit, the controller stops waiting for its result and keeps the window responsive. The
  native code goes on running. A late result from that call is not admitted.
- **Hang.** The page offers *Stop add-in process*. The supervisor ends the surrogate (a job
  object holds it and every child process it starts) and does not block on its teardown: a live
  worker can prevent a physical drain indefinitely.
- **Crash.** The supervisor sees the surrogate end, marks it failed, and rejects every message
  from that incarnation.
- **Outcome of each open call.** An invocation whose result was not accepted reports exactly:
  "The add-in process stopped before this result was accepted. External work may have completed.
  Retrying will run the add-in again." Results already accepted keep their status, and the failure
  is reported once for the add-in rather than against every result it ever gave.
- **No automatic replay.** Restart is an explicit action. Nothing re-runs an invocation, macro,
  event or RTD subscription by itself, because any of them may already have had external effects.

## 7. The controller's durable copy and the commit point

The controller keeps a recoverable checkpoint of the document and an ordered record of resolved
authored changes. It is not a second workbook implementation and it never calculates.

**DNA Calc.** `calc-host` stays the only semantic writer. For each authored change it resolves and
stages the change, then sends an envelope with a commit ID, the predecessor revision, the payload
digest and its incarnation (its write lease). The controller checks the envelope and appends the
resolved data durably. **That append is the commit point.** Only then does the session install the
revision and acknowledge it. A retransmission with the same commit ID returns the stored receipt.
A changed payload under that ID, a stale predecessor or a retired writer is rejected. A lost reply
is settled by commit ID. It never runs the originating macro or event again. When a surrogate
is replaced, the old write lease is retired before the new one is admitted.

**OneCalc.** The authored state is small: the opened file's bytes, the formula text the person
typed, and the list of loaded add-in paths with their load order. The controller records a change
to any of these before it forwards the request. That record is the commit point. Values are not
journaled: they are recalculated.

**Recovery.** A replacement surrogate starts in recovery mode with every route into user code
held back. It restores committed authored state and registration metadata. It does not load any
add-in image, initialise a managed or Python runtime, activate COM, or raise `xlAutoOpen`,
`Workbook_Open` or automatic recalculation. Each of those can run user code (`DllMain`, static
initialisers, imports), so blocking only the open events is not enough. The page then shows what
was restored and offers *Load add-ins and recalculate* as one explicit step. VBA and Python globals,
native heaps, open handles, COM identities and suspended continuations are not restored. Work that
was not committed may be lost, and the page says so.

## 8. Async, RTD and unload

- **Async tokens** carry the incarnation and the provider generation on top of today's pending
  key. A completion is checked against all three before its result operand is read. A late
  completion from a retired incarnation is dropped.
- **RTD** servers, the update object and their COM references live on the surrogate's STA. Topic
  leases include the provider generation. Input admission stays with the session (unchanged). A late
  signal from a retired server can be received for cleanup, but it never becomes a new input
  because a numeric topic ID was reused.
- **Unload is logical, and the image stays resident until the process ends.** Unloading an add-in
  deactivates it: `xlAutoClose` only (dc-6332.10, from every Excel trace), registrations retired,
  RTD and events disconnected, frees completed on the owner. The image stays loaded. A completion
  receipt, a managed unload call or the disappearance of formula references does not prove that no
  thread, timer, queued message, class factory or COM reference still runs module code. `FreeLibrary` runs
  only for a module with a proven quiescence protocol, and our own fixtures are the only such
  modules today. The real reload is a new surrogate, followed by an explicit load.
- **Close** has two phases. First the supported close events run. After the close commits, new
  roots, async and RTD admission are refused, and cleanup (frees, finalisers, unadvise) runs on
  the owner while its code, tables and windows still exist. The cleanup keeps the FreeOnly
  restrictions. If an add-in does not cooperate, the supervisor ends the surrogate and reports that
  graceful cleanup did not finish.

This is what dc-snz.21 and dc-snz.19 need. xlOil's native reload crashes Excel, and Native AOT does
not support runtime removal. Neither is repaired in-process: a *reload* there means ending the
surrogate and loading into a new one, with the recorded difference from Excel kept.

## 9. Authority and trust

A surrogate contains failures. It is not a sandbox. It runs as the same user with the same file
system and network access. Macro security levels (Off, Guarded, Unguarded) keep their meaning and
are not tied to which process code runs in. A Guarded grant never loads native code, which already
holds today. Code that shares a surrogate is one trust domain. A restricted OS process policy
would be a separately specified capability and is not part of this design.

## 10. Where the code goes

- Transport, launch, the job object and the supervisor go in `calc-platform-desktop`, beside the
  window chain and WebView2 host. It stays free of product dependencies: it moves envelopes and
  bytes and never decodes a payload.
- The envelope and message checks are one small native-only module. Each application uses it to
  wrap its existing seam (`dna-onecalc` `Link`, `dna-calc-model` `WorkbookTransport`).
- The surrogate role is a mode of each desktop executable (`dna-onecalc-desktop`,
  `dna-calc-desktop`). The dependency ceiling is unchanged: OneCalc's surrogate reaches no
  `calc-host` and no `calc-engine`, and dna-calc-desktop does not depend on dna-onecalc.
- `calc-engine`, `calc-fml` and `calc-func` get no XLL or IPC dependency.

## 11. Stages and what shows each works

**Stage 3: OneCalc in a surrogate.** The page and window in the controller; `Session` and
`AddinHost` in the surrogate over the `Link` seam.

- The existing OneCalc acceptance (`cargo xtask app xll-accept onecalc`, m4 and m5 plans) passes
  with the same counts in both transports.
- Fault fixtures (a disposable `BB` export: access violation, infinite wait): the window stays
  responsive, the open call reports the unknown-outcome message, restart is explicit, and no
  invocation runs again by itself.
- Old-incarnation async completions and RTD signals are delivered after a restart and none is
  accepted; no pointer crosses (checked in the transport's decoder).
- Startup, per-call and per-callback overhead is measured against the in-process mode and
  reported, not targeted.

**Stage 4: the DNA Calc session in a surrogate.** It needs Stage 3 working and the commit point of
section 7.

- Open, edit, calculate, VBA/XLL read, save and reopen work through the surrogate with the same
  corpus and page acceptance counts as in-process.
- Failure is injected before and after the durable append, with a lost commit reply, and before and
  after file replacement on save. Every acknowledged edit survives, a retransmitted commit ID is
  recorded once, and no macro or event runs again.

Stop after one session process works. There is no per-DLL process manager and no multi-runtime
routing until a concrete incompatible pair is measured (dc-6332.13, D.19).

## 12. Open, and where it is measured

- The order of Excel's close hooks, RTD termination, finalisers and events (D.13, D.14).
- The lease witness: a worker blocked after `xlAsyncReturn` while unload is requested (D.21, lease
  part; dc-6332.13).
- Whether a framework needs any controller-side UI (ribbon, custom task panes) before Stage 3 can
  ship for it. Until it has a bridge, each such feature stays a typed refusal naming its bead.
- .NET and Python coexistence in one surrogate (D.19 matrix).
