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

Ownership requires a new Excel PID absent from the prelaunch list, its start time and Excel image.
PID 3144 and `/restore` processes are refused. Cleanup checks that exact tuple. Only the host's
serialized deadline may stop an active job and its owned Excel. A client can cancel its own
still-queued request; it never terminates a process based on a stale status read. No process-wide
Excel kill or security policy or registry change is used. The existing workbook capture worker
temporarily sets the owned application's `AutomationSecurity` to 3 to disable macros; reset
restores the recorded value. Witness, iteration and event jobs preserve and check that value.
Launch attempts are persisted before COM activation; a failed activation can therefore be counted
without claiming a confirmed new PID.

`run-packet.ps1 -PacketDirectory <packet> -OutputDirectory <new-directory>` generalizes the
supplied RUN-R16 runner. It validates packet hashes, freezes product predictions through the
existing witness reader, retains new canonical readings and raw worker files, and reports launches.
`-CapturePending` also uses the existing sectioned catalogue capture. The adaptive R19 script
uses the same route without modification and inherits the selected session directory.

The eng9 runner uses a fresh-required job per pure condition and resets/verifies the baseline
between ten separately seeded workbook trials. It retains trial order and explicitly records
shared process memory. P06 event lifecycle trials each get a fresh process. Inputs must match
their frozen source/preparation hashes, declared formulas and seeds, live initial values and
post-open application settings before a calculation command runs. Preparation failures retain
their error and setting readback and do not discard other prepared conditions. The result
distinguishes completion of planned jobs from the broader packet's unrun requirements.
