The lane's Excel session host runs the existing capture, witness and XLL scripts with an owned
Excel application. `cargo xtask capture`, its corpus capture route, and `cargo xtask witness read`
submit jobs automatically. `crates/calc-xll-reference/run.ps1` submits its Excel side; its DNA Calc
side continues to use the existing native process runner. No second calculation or oracle route
is introduced. PowerShell 7 on Windows and Excel are required.

Set `DNACALC_EXCEL_SESSION` to select a lane's queue directory; the default is this clone's
`target/excel-session`. `submit.ps1 -Script <path> -Parameters <JSON> -Environment <name>` starts
the host when necessary. Job scripts receive `OwnedExcel` and must return rather than exit,
quit Excel or release its application RCW. They may close their own workbooks. The host keeps
one owned process alive until shutdown, crash, baseline drift or a fresh-required job. Stop it
with `submit.ps1 -Stop`; inspect it with `submit.ps1 -Status`.

The file queue retains every request, worker log and receipt. Among waiting jobs, ordinary jobs
run first, grouped by environment, then fresh-required jobs grouped the same way. Use
`-FreshRequired` for process lifecycle, first-open, unmanaged registration and other jobs whose
condition requires a new process. Such conditions cannot be inferred from a formula. Private COM
and XLL reload tests are marked by the reference runner. No global Excel registration is changed.

Before and after each job the host checks a recorded baseline: an empty saved sentinel workbook,
application calculation and iteration settings, events, alerts, security, visibility, interaction,
separators, status bar, clipboard mode, MTC state/count/mode, registered functions and the installed,
open and connected add-in inventories. Reset closes owned workbooks and restores application
settings; it never installs or disconnects an add-in. The sentinel is closed before invoking the
job, preserving its first-workbook behavior. A reset that differs from the baseline refuses the
job and recycles the owned process. Raw output survives. A failed job whose baseline is intact
does not require a new launch. These checks do not prove absence of hidden add-in state.

Every fiftieth ordinary job is rerun as a fresh-required measurement. Witness rows, capture cell
observations or XLL controller result observations are compared, with both raw runs retained.
Differences are reported as leakage observations, never used to change an answer. Volatile values
can differ legitimately. The fresh repeat's clean process is kept for later jobs. Receipts count
all launch attempts, including fresh repeats and recovery; launch records survive host restarts.

The exact current capture worker also reuses one hash-verified script copy within the host.
This avoids PowerShell retaining a separately parsed copy for every workbook. Other scripts and
unknown worker versions keep their submitted paths. Receipts retain the submitted and executed
paths and hashes; a modified cached copy refuses execution. Every workbook still receives the
same baseline checks and its own unchanged request, raw result and saved package.

Ownership requires a new Excel PID absent from the prelaunch list, its start time and Excel image.
PID 3144 and `/restore` processes are refused. Cleanup checks that exact tuple. Only the host's
serialized deadline may stop an active job and its owned Excel. A client can cancel its own
still-queued request; it never terminates a process based on a stale status read. No process-wide
Excel kill or security policy or registry change is used. Capture, timing, witness, iteration and
event workers preserve and check the application's existing `AutomationSecurity`. The host
refuses drift and recycles its process without assigning that setting.
Launch attempts are persisted before COM activation; a failed activation can therefore be counted
without claiming a confirmed new PID.

`run-packet.ps1 -PacketDirectory <packet> -OutputDirectory <new-directory>` generalizes the
supplied RUN-R16 runner. It validates packet hashes, freezes product predictions through the
existing witness reader, retains new canonical readings and raw worker files, and reports launches.
`-XtaskExecutable` selects an already built command harness and records its hash; this avoids
overlapping Cargo use while the per-commit check runs in its own target directory.
`-CapturePending` also uses the existing sectioned catalogue capture. The adaptive R19 script
uses the same route without modification and inherits the selected session directory.

An oracle's optional `oracle.entries.json` uses the capture worker to attempt explicitly authored
Formula/Formula2 entries at their original callers. The source workbook stays unchanged; entry
errors retain their inventory cells without invented Excel values. They count as missing results.
The existing file-form serializer validates authored versus stored text before Excel runs. The
capture identity includes the ordered entry declarations, so changed or removed entry inputs
make an old capture stale. Comments and build provenance do not change that identity. These
fixtures use whole-workbook captures; splitting them into ordinary sections is refused.

The eng9 runner uses a fresh-required job per pure condition and resets/verifies the baseline
between ten separately seeded workbook trials. It retains trial order and explicitly records
shared process memory. P06 event lifecycle trials each get a fresh process. Inputs must match
their frozen source/preparation hashes, declared formulas and seeds, live initial values and
post-open application settings before a calculation command runs. Preparation failures retain
their error and setting readback and do not discard other prepared conditions. The result
distinguishes completion of planned jobs from the broader packet's unrun requirements.

`run-iteration-packet.py --resume` verifies the previous delivery hashes, frozen inputs and
completed trial receipts before continuing. It uses the existing trial requests and refuses
to overwrite a raw result. Remaining conditions get trials 1-3 before trials 4-10. Pure trial
groups share one fresh process, with baseline resets; each P06 trial uses a fresh process.
Earlier and resumed groups use different processes. Every receipt and raw result is retained,
and the resume inventory lists remaining trials and launch counts. `--dry-run` audits and
writes the proposed remaining trial groups without starting Excel.

The host always preserves `AutomationSecurity`, including avoiding assignments of its existing value. It
records this policy in its status and receipts. A different value refuses reset and recycles the
owned process; it is never restored by assigning the baseline. Clients
refuse an already running older host without this policy.

`submit.ps1 -NoSaveBaseline` selects an unsaved blank baseline for queues that must never save
workbooks. Use a dedicated session directory; mixing saving policies in a running host is refused.
Unsaved workbook names are normalized in baseline comparisons because Excel increments them.

Status readers permit atomic replacement while their handle is open. Metadata publication retries
temporary Windows sharing/access errors for a bounded interval, retaining the old file and the
complete temporary JSON until replacement succeeds. Other failures still stop the job.
`--reviewed-unstarted <JSON>` lets the iteration resume audit retain a failed startup and submit
its still-unrun trials. Each review binds the receipt, condition request, frozen trial requests
and exact host source, with no worker log, pre-worker baseline, result or Excel binding in the job receipt. These
failed attempts remain in the delivery and launch totals; they never count as completed trials.

For new pure conditions, pass one or more `--conditions-file <JSON>` arguments with
`--profile eng10`. Each file supplies a `conditions` array with exact formulas, typed stored
seeds, Formula2 form and null expected answers. The runner uses just those conditions, with
ten fresh owned processes each. `--dry-run` checks the planned counts without starting Excel.

Raw capture cells now retain the runtime type of the same Value2 getter that supplied their
canonical value, plus Formula, Formula2, HasArray and SavedAsArray getter results. Immediate
and reopened readings keep their own metadata. A numeric NaN stays a numeric bit pattern;
worksheet errors keep their observed error codes, and a failed metadata getter stays unread.
