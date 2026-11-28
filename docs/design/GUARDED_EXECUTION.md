# Guarded execution

Bead `dc-y46aj`, lane fbs1, 2026-11-27. It answers two questions:

1. Six independent security reviews of the Touchstone VBA policy failed (rw14 to rw19). Each fix
   closed what the last review found. What makes the policy hold by construction?
2. Is the same idea a product feature for DNA Calc: running a workbook's macros and add-ins safely,
   where Excel offers only a trust prompt?

It builds on [CODE_EXECUTION_POLICY.md](../architecture/CODE_EXECUTION_POLICY.md) (dc-8ovd: the
levels, origin and reach, the capability names, the CalcVBA hosting interface) and does not repeat
it. Nothing here loosens a setting. Touchstone VBA approvals stay frozen until the review in
section 3.6 passes.

## 1. The insight

**Safety has to come from what can be said, never from recognising what is bad.**

Every Touchstone finding is one mistake made again: the policy receives text in a language that
another program will run (VBA source, an Excel formula, a workbook package), reads it with its own
reader, and decides whether it looks harmless. That cannot be made to hold, for four reasons, and
every one of the six reviews found at least one of them.

| Cause | Why inspection fails | Findings (review) |
|---|---|---|
| **A. Two readers.** The checker reads the text one way and Excel another. | The checker would have to be Excel's lexer and parser, exactly, for every build. | `Name "a" As "b"` passing as an object named Name (rw14); a module named `Name` (rw15); quote desync in layout formulas, CFBF and MS-OVBA parser differences (rw15); `_xleta.` prefixes, Unicode identifiers, `U+001C..1F` as whitespace (rw19) |
| **B. Names are not calls.** The language can call a thing without naming it where the checker looks. | VBA and the formula language both have first-class indirection. Listing dangerous names never closes it. | A declared name shadowing a global; `Evaluate`, `Run`, `ExecuteExcel4Macro` (rw14); formulas written through `.Value` (rw15); a bare function passed to `MAP` or `BYROW` (rw18); `LET`, call-of-a-result, defined names, `MAKEARRAY` (rw19) |
| **C. Two deciders.** The policy is written twice, in Python and in C#. | Two programs never agree on every input. Each disagreement is an input one of them admits. | Kelvin sign case folding, lone surrogates, `names: null` (rw18); whitespace classes, a 401-digit integer read as Infinity, sheet-name casing (rw19) |
| **D. Checked is not used.** What was checked is not the thing that runs. | A check on one representation says nothing about another. | Uploaded workbook bytes never compared with the checked source (rw14); the digest leaving out the template (rw18); the raw request dictionary reused after the check, which wedged the broker (rw19) |

A deny list fails on B. The "positive grammar" of rw15 fixed the deny list and still failed, because
a grammar over received text is still inspection: it fails on A and C. The probe templates
(dc-erfm3) were the first constructive step. Once their two defects were fixed (the old free-source
route was still open, and one parameter type was written without its check), rw19 found no bypass in
the template route. It found its bypasses in the one place that still took text: layout formulas.

The alternative is construction. The requester sends **data from a closed vocabulary**. The trusted
side **writes** every byte of code that Excel reads, from that data, with one printer. Then:

- A disappears: there is no received text to read differently. Excel reads only what the worker
  printed.
- B disappears: indirection needs code, and the requester supplies none.
- C disappears: one program decides. A second copy can exist for early error messages and it has
  no authority.
- D disappears: the worker runs the value it built, and the digest is taken over that value.

The same sentence, for a runtime we own, reads: **the code can only reach what it was handed.**
That is CODE_EXECUTION_POLICY's Guarded ("a model, not a deny list"), which reached the same
conclusion from its first real failure (dc-g4mj3). Touchstone and Guarded are the two halves of
one rule:

| | Who interprets the code | What we can constrain | Rule |
|---|---|---|---|
| **Touchstone** | Excel, with the Windows account's full rights | The input | Construct the input. No requester text is ever parsed as code. |
| **DNA Calc Guarded** | CalcVBA, QuickJS, our formula engine | The authority | Construct the authority. No ambient reach; every outside effect is a call on something the host handed over. |
| **Native add-ins (XLL, COM)** | The CPU | Neither, in process | Contain the process or do not load. Never inspect. |

## 2. Three rules that follow

**R1. No text from a requester reaches a position where an interpreter parses it as code.**
A request value may reach only a *literal position*, through a typed writer that cannot produce
anything else: a number through a binary64 value, text through a cell formatted as text or a quoted
VBA string literal with its escape, a sheet through a lookup in the case's own sheet list, an enum
through a closed list.

**R2. One authority, one decoding.** The request is decoded once, by the program beside the
interpreter, into a typed value. Unknown fields, wrong types and out-of-range values refuse there.
Everything after that step uses the typed value, never the received bytes or dictionary. The
evidence digest is computed from the typed value.

**R3. Review the function, not the instance.** What a person reviews and approves is a template
together with its parameter types, and the claim reviewed is: *for every value of these types, the
expansion is safe.* That claim is finite and checkable. "This source looks safe" is not.

## 3. Touchstone: the design

What exists today (after rw18): a `_VBA` case names a reviewed template with typed parameters; free
module source is refused; the worker builds the workbook itself; text cells are written with the
text number format; the digest binds template, parameters and module hashes. Three things still
break the rules, and they are exactly where rw19 failed.

### 3.1 Formulas come from the template (closes dc-90xzd by removing its surface)

Today a case's `sheets[].cells` may hold `{"type": "formula", "formula": "<text>"}` and `names`
holds formula text. Two filters read that text. This is the last inspection in the route.

Change: a `_VBA` request carries **values only** in its layout: number (bits), text, boolean,
error code, blank. `names` is not a request field. A probe that needs a formula or a defined name on
the sheet (a `Worksheet_Calculate` probe, a `Range.Formula` readback) declares it in its template:

```json
"smoke": {
  "modules": [...],
  "layout": [
    {"sheet": "{{target_sheet}}", "at": "C1", "formula": "=A1*2"},
    {"name": "Rate", "refers_to": "={{target_sheet}}!$A$1"}
  ],
  "params": {...}
}
```

Template layout formulas are fixed text, reviewed with the template and covered by its pin. A
parameter may appear only as a sheet reference or a literal, rendered by the same typed writers as
in the modules. `_check_vba_layout_formula`, `VbaPolicy.CheckLayoutFormula`, `vba_layout_scan`, the
blocked-function and higher-order lists, and the procedure-name collision check are deleted: there
is no request formula left to scan.

The values lane (`_CASE` and the function groups) is unaffected. It was already constructive: the
worker builds `=F(args)` from a function id and typed arguments.

### 3.2 The worker is the one authority (closes dc-k68ne)

The C# worker decodes the group into a typed `VbaCase` record with a strict decoder: the envelope
fields `f`, `k`, `n`, `o`, `i` typed and ranged (integers as integers, never through `double`), no
unknown field at any level, every string ASCII-checked where the type says so. The worker then uses
only the record. The broker never indexes into a `_VBA` group it did not decode the same way: it
treats the group as opaque and asks the worker's decoder.

`policy.py` keeps no VBA grammar, template expander or formula scanner. For early refusal the
broker and the client call the worker's own decoder, which already builds and runs on Linux
(`excel-worker --check-vba-policy`, used by the test suite as `TOUCHSTONE_VBA_POLICY_WORKER`). One
implementation means there is nothing to hold in parity. A parity test becomes a test that the
Python side has no opinion.

### 3.3 The grammar moves to review time

`VbaGrammar` stops running on requests. It runs once, when a template is added to the catalogue
(`cargo xtask` template admission), over the template text with each placeholder treated as a
literal token of its type. It checks R1 mechanically: every placeholder stands in a literal
position, never as an identifier, a statement or part of one. A template that fails is not pinned.
At request time the route is: find the template, check its pins, type-check the parameters, print.
No parser sees requester data.

### 3.4 Approve templates, record cases

Approval attaches to a template's pins and its reviewed claim (R3), recorded beside the catalogue
entry: who reviewed it, the claim, the parameter types. A case digest stays as the identity of an
observation for the bank. It is no longer the unit of approval, because every instance of an
approved template is covered by the claim. This is what lets the lane answer questions at volume
without a person approving each case.

### 3.5 What is left, stated plainly

- **A wrong template.** A reviewed template can still be unsafe for some parameter value. That is
  a review error on a small, finite text, and 3.3 checks the mechanical half of it.
- **Excel itself.** A fixed template formula or statement can behave differently on another Excel
  build. Pins include the Excel build family the review covered.
- **Containment is separate and stays.** The isolated worker, the private desktop, the dialog
  guard, read-only open, close without saving, the deadline and the post-run fingerprint do not
  depend on the policy. They bound the damage of the two cases above.

### 3.6 Order and exit

1. 3.1, then 3.2, then 3.3 and 3.4 (beads in section 7). Each removes code from the request path.
2. A seventh review, with a different brief from the first six: not "find a bypass" in a filter,
   but "name a byte of requester data that reaches Excel's VBA or formula parser other than through
   a typed writer". The reviewer gets the list of writers. If the list is complete and each writer
   is total, the review passes.
3. Only then does the coordinator consider unfreezing approvals. This document does not unfreeze
   anything.

## 4. DNA Calc: what the same idea makes possible

In DNA Calc we own the interpreters, so we construct the authority instead of the input.
CODE_EXECUTION_POLICY sections 2 to 4 are that design for VBA: origins, reach per member in the
object-model descriptor, one `decide`, mediated facets, an audit. Two additions complete it, and
then three product features fall out that Excel cannot offer.

### 4.1 Two additions to the policy

**Formulas are code.** The Touchstone bypasses of rw18 and rw19 all went through formulas, and the
same door exists in a Guarded macro: `Range.Formula = "=..."`, `Evaluate`, a defined name. So the
formula engine's outside reach takes the same `decide` as a VBA statement's: COM RTD servers
(`com.activate`), external workbook links (`fs.read`, `network`), and later `WEBSERVICE` and `IMAGE`
(`network`, dc-fetch-f0tp), `CALL` and `REGISTER.ID` (`ffi`). The engine already takes its outside
services from the caller; under Guarded the host hands it mediated ones. A workbook with no macro
at all gets the same protection, which Excel's macro setting never gave.

**The world is closed, so a run is a function.** With every outside reach mediated, a Guarded
macro run has exactly these inputs and outputs: the workbook, the dialogs the person answered, the
granted capabilities' calls (each in the audit), and the resulting workbook. Nothing else exists.
That one fact is what the features below rest on.

### 4.2 Permission by capability, not a trust prompt

Excel asks "enable macros?" before the code has done anything, about all of it, forever. Guarded
runs the macro and stops at the first outside reach with a typed question: *this workbook wants to
read files in its own folder*; *this workbook wants to call `https://api.example.com`*. The person
grants that capability for that workbook, once or for the run, or refuses and the macro receives
the ordinary VBA error. Triage (CODE_EXECUTION_POLICY 2.6) shows the list before the first run.
Most macros move cells and ask for nothing.

### 4.3 A preview of what a macro will change

Because a Guarded run without grants has no outside effects, it can run on a copy and be thrown
away. `ModelRoot::pin` already snapshots the workbook in O(1). The preview is:

1. fork the session state (the pinned root, and the VBA project's runtime state);
2. run the macro on the fork;
3. show the difference in the terms the skin architecture already has for an agent proposal
   (`Preview.writes`: cells, formulas, names, sheets, with counts), plus the audit;
4. **Apply** adopts the fork as the new state, as one history entry. It does not run the macro
   again, so the person gets exactly what they saw, including `Rnd`, `Now` and dialog answers.
   **Discard** drops the fork.

A run that used a granted outside capability can still be previewed, and the preview says which
calls already happened and cannot be taken back.

### 4.4 One undo across the run

SKIN_ARCHITECTURE section 17.1 lists seven hard problems for undoing a macro run, and promises
only Excel's behaviour in v1 (owner ruling D4). Guarded changes which of them are hard:

| Problem (17.1) | Under Guarded, no grants |
|---|---|
| 1. VBA runtime memory diverges from restored cells | We own the interpreter: the fork in 4.3 includes it. Needs a CalcVBA project-state snapshot (cost, section 6). |
| 2. External side effects | There are none. With grants, they are the audit's list: `DocumentOnly { external_effects }` is filled from it. |
| 3. Event re-entrancy on undo | Still needs the Excel measurement on Windows. |
| 4. Session against document changes | All made through the one object model, so all journaled. |
| 5. `Application.Undo` and `OnUndo` inside macros | Unchanged: implement as Excel does. |
| 6. Faulted runs | A faulted run leaves Excel's partial writes, as Excel does. The undo entry restores the pre-run state. |
| 7. Replan cost on large workbooks | Unchanged: measured, not assumed. |

So the honest statement is: **undo across a macro run is reachable for Guarded runs without
outside grants, and is the place to try it first.** For Unguarded runs it stays as D4 says.
The committed result of a run is always what Excel would produce. Preview and undo are host
features around the run and add nothing to the calculation.

### 4.5 One policy across VBA, JavaScript, XLL and COM

The capability names and the audit are shared (CODE_EXECUTION_POLICY 2.2). What differs is how the
rule of section 1 applies:

- **VBA and embedded JavaScript:** our interpreters. Guarded by constructed authority. Preview and
  undo apply.
- **Office.js task panes:** the browser sandbox is the interpreter, and the Office.js projection is
  the only door. Its batches arrive as data (`BatchRequest`), which is already R1.
- **XLL and COM add-ins:** native code has the person's rights and cannot be mediated in process,
  so Guarded never loads it (CODE_EXECUTION_POLICY section 1). The only safe route is containment:
  the add-in surrogate in an operating-system sandbox, where the wire (`calc-surrogate-wire`) is the
  one thing it can say. That is the same rule again, and it is real work on Windows. Until it
  exists, the product says "Blocked or Unguarded" for native add-ins and nothing softer.

## 5. Is it a differentiator?

Yes, for three groups, and it should be said on `/automation/` only as each part ships.

- **People who receive workbooks.** Since 2022 Excel blocks macros in files from the internet
  outright. The practical choice for a received `.xlsm` is "do not run it" or "run it with my whole
  account". Guarded with a preview is a third answer nobody else has: *run it, see what it would
  do, keep or discard.*
- **Reviewers and auditors.** The audit and the preview diff are a record of what a macro does to
  a model, produced by running it rather than reading it.
- **Agents.** An agent can run a workbook's macro in a Guarded fork and read the diff, with no way
  for the macro to reach the machine. That makes "the agent may run macros" a setting a person can
  switch on.

Excel cannot follow cheaply. Its VBA calls the operating system directly (`Declare`, `CreateObject`,
file statements); authority is ambient, and removing it breaks the macros people rely on. We can
because the interpreter and the object model are ours, with every outside reach already behind a
host facet.

What would make it false: if most real macros need outside reach, Guarded is a demo. That is a
number we do not have. Section 7 files the measurement: over the VBA corpus, the share of projects
whose procedures run to completion under Guarded with no refusal, and the capabilities the rest
ask for. The story is told with that number or not at all.

## 6. What it costs

- **The object model must be complete about reach.** Every member that reaches outside needs its
  `reach` in the descriptor (CODE_EXECUTION_POLICY 4.5 step 5). A missed member is a hole. The
  defence is the default: a member with no entry is refused in Guarded unless the descriptor
  review marked it pure.
- **CalcVBA must be able to snapshot a project's runtime state** (module variables, statics,
  object references into the host). Without it, preview and undo are sound only for a run that
  starts from a reset project. This is the largest new piece of work.
- **Resource use is a reach too.** A step budget exists (`DEFAULT_STEP_BUDGET`); memory and wall
  time limits are needed before "safe" covers a hostile loop.
- **Dialogs can lie.** A `MsgBox` or UserForm that asks for a password is an outside effect through
  the person. Guarded shows macro dialogs in a frame that names the workbook as the speaker.
- **The workbook itself is in reach.** Guarded protects the machine and the network, not the
  workbook's own data from its own macro. The preview is the control for that.
- **Fidelity.** A refused capability is a VBA error the macro may not handle. Each one is a typed
  refusal naming the capability, counted like any other refusal.
- **Native add-ins get nothing from this** until the sandboxed surrogate exists.

## 7. Beads

Touchstone (related to dc-ruive; approvals stay frozen throughout):

1. `dc-5121n`: `_VBA` requests carry values only; formulas and names come from the template's pinned layout;
   delete the layout-formula scanners (3.1). Removes the surface of dc-90xzd.
2. `dc-4rw9n`: The worker's strict typed decoder is the one authority; `policy.py` loses its VBA grammar,
   expander and scanner and calls the worker's decoder; the broker treats `_VBA` groups as opaque
   (3.2). Covers dc-k68ne.
3. `dc-un9a4`: `VbaGrammar` becomes template admission with a placeholder-position check; approvals attach to
   template pins with a recorded claim (3.3, 3.4).
4. `dc-wj3pu`: Seventh review, on the brief in 3.6.

DNA Calc (related to dc-euei and dc-8ovd):

5. `dc-hk7im`: Formula routes take the policy decision under Guarded: RTD servers, external links, and each
   later outside function (4.1), with a test that a Guarded macro writing such a formula is decided
   and audited.
6. `dc-a6ee2`: Measure Guarded on the VBA corpus: share of projects that run clean, capabilities the rest ask
   for (section 5).
7. `dc-t1l3b`: Guarded preview experiment: fork, run, diff, apply or discard, on the Guarded action-plan
   macros, including the CalcVBA state snapshot it needs (4.3, 4.4). Feeds SKIN_ARCHITECTURE M8.
