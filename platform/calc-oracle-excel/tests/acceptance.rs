//! Acceptance cases owned by the calc-oracle-excel package, each registered by its exact name in
//! `platform/calc-oracle-excel/acceptance.json`.
//!
//! `infra_m1_2__untrusted_workbook` has two parts.
//!
//! - The isolation part, on Windows (the `windows` and `excel` routes, and a plain `cargo test`
//!   on Windows): [`run_isolated`], the isolation every capture runs under, drives
//!   `tests/fixtures/standin-worker.ps1`, a stand-in worker that starts a stand-in Excel (a
//!   PowerShell process that writes its own PID) through WMI, so that, like an Excel that COM
//!   activates, the stand-in is not a descendant of the worker, and reports it on the worker
//!   protocol. A sentinel process of the same image, started before the run, stands for another
//!   Excel, the owner's. With a worker that hangs, the run must time out, and afterwards no process
//!   whose command line carries the stand-ins' token may exist while the sentinel still runs, and
//!   the run must report its Excel as terminated; with a worker that exits and leaves its Excel
//!   running, the run must terminate that Excel and only it, and report it as terminated (not as
//!   exited); with a worker that reports the sentinel's PID, the run must refuse it and terminate
//!   nothing of it. What is running is read from the operating system (Win32_Process), not from
//!   what the adapter reports. Only this part runs on the windows route: the macro and link half
//!   is the excel part.
//! - The excel part, only on the `--platform excel` run (the runner sets
//!   `DNACALC_ACCEPT_PLATFORM=excel`), which any agent may run on the owner's machine (AGENTS.md,
//!   Excel on this machine): live captures of the untrusted fixtures
//!   through [`capture`], the route `cargo xtask capture` takes. `untrusted-link.xlsx` reads
//!   `link-target.xlsx` (copied beside it) through an external link whose cached value is 111
//!   while the file holds 333; the capture must observe 111 and 112 (=A1+1) after the full
//!   rebuild, in the package Excel saved (its cell caches and its external link part, read here
//!   from the file on disk, and the record's saved caches must agree) and after reopening that
//!   package (which only Excel can read, so that part comes from the record), so the link was not
//!   refreshed at any point. `untrusted-macro.xlsm`
//!   holds a VBA project whose `Workbook_Open` and `Auto_Open` would set Sheet1!A1 to 100 and
//!   write `macro-ran.txt` beside the workbook, and whose user-defined function `MarkUdf`, called
//!   by Sheet1!C1, would write the same marker and return 7. Events off already keep
//!   `Workbook_Open` from running and `Workbooks.Open` never runs `Auto_Open`, so those two alone
//!   cannot show that macros are disabled; the full rebuild does call `MarkUdf` unless VBA is
//!   disabled. The capture must observe B1 (=A1*2) as 2 and C1 as a worksheet error (not 7),
//!   before and after the save and reopen and in the saved file's caches read from disk, and no
//!   `macro-ran.txt` may exist anywhere in the capture's work directory or beside the fixture.
//!   Dynamic-array Excel may save `=MarkUdf()` as a single-cell dynamic-array formula; the
//!   capture records it as an ordinary formula flagged as dynamic.
//!
//! The excel part needs an Excel instance with no add-in open in it: the worker reads every
//! add-in's state, switches none off, and refuses the capture, naming each, when one is open in
//! the instance (or its state cannot be read) or XLL functions are registered. A connected COM
//! add-in does not refuse it: the record lists each in its setting `com_add_ins_connected`
//! (which calc-verdict counts), and each installed add-in in `add_ins_installed`, by name only;
//! the case checks that no add-in setting holds a path and prints them. That refusal and that
//! record are exercised without Excel by the worker unit tests, with a stand-in Application. The
//! case also checks that Excel opened each fixture read-only (the record's `opened_read_only`),
//! the third argument the worker passes Workbooks.Open through COM's own call.
//!
//! The macro half's positive control is the fixture as authored: its saved cache holds 7 for
//! Sheet1!C1, the value MarkUdf returns when VBA runs, so the function exists and ran when the
//! fixture was authored. The capture must then give `#NAME?` there, exactly.
//!
//! The macro workbook cannot be authored honestly without Excel, so it is authored once in Excel
//! (in the UI or through COM automation), by these steps, which the case names when the file is
//! absent:
//!
//! 1. A new workbook with Sheet1!A1 = 1 and Sheet1!B1 = `=A1*2`.
//! 2. In the VBA editor, in `ThisWorkbook`:
//!
//!    ```text
//!    Private Sub Workbook_Open()
//!        MarkRan "Workbook_Open"
//!    End Sub
//!    ```
//!
//!    and in a new standard module `Module1`:
//!
//!    ```text
//!    Sub Auto_Open()
//!        MarkRan "Auto_Open"
//!    End Sub
//!
//!    Sub MarkRan(source As String)
//!        Dim f As Integer
//!        f = FreeFile
//!        Open ThisWorkbook.Path & "\macro-ran.txt" For Append As #f
//!        Print #f, source
//!        Close #f
//!        ThisWorkbook.Worksheets("Sheet1").Range("A1").Value = 100
//!    End Sub
//!
//!    Function MarkUdf() As Double
//!        Dim f As Integer
//!        f = FreeFile
//!        Open ThisWorkbook.Path & "\macro-ran.txt" For Append As #f
//!        Print #f, "MarkUdf"
//!        Close #f
//!        MarkUdf = 7
//!    End Function
//!    ```
//!
//! 3. Save it as an Excel Macro-Enabled Workbook at
//!    `platform/calc-oracle-excel/tests/fixtures/infra_m1_2/untrusted-macro.xlsm` with A1 still 1.
//! 4. Enter `=MarkUdf()` in Sheet1!C1 (it shows 7 and writes `macro-ran.txt` beside the
//!    workbook), save again and close without reopening it there (opening it with macros enabled
//!    runs `Workbook_Open`).
//! 5. Delete `macro-ran.txt` from the fixture directory: the case refuses to run while one lies
//!    there.
//!
//! Until the workbook exists, the excel part reports not run.
//!
//! `dc_xb26__spill_shape` has two parts.
//!
//! - The stand-in part, on Windows (the `windows` and `excel` routes, and a plain `cargo test` on
//!   Windows): the case runs, with cargo in a target directory of its own and by their exact
//!   names, the crate's unit tests that hold the spill scenarios, and requires each to run and
//!   pass. They are unit tests because the conversion of a worker's result is crate-private (the
//!   record it gives is stamped a live capture). The worker scenarios (`src/worker_tests.rs`) call
//!   the capture worker the adapter embeds with `-Request` and `-Result`, the parameters the
//!   adapter passes it, from the guard of `tests/fixtures/standin-harness.ps1` (which creates no
//!   COM object, so no Excel starts) against the stand-in Excel `tests/fixtures/standin-excel.ps1`
//!   on `tests/fixtures/worker/spill.xlsx`; [`capture`]'s own start of the worker (its
//!   `powershell.exe -File` under `run_isolated`) is not part of them. `spill.xlsx` holds
//!   constants in A1:A2, Sheet1!B1 =SEQUENCE(3,2) saved as a dynamic-array formula spilling over
//!   B1:C3, D1 an ordinary formula and E1 a dynamic-array formula over E1 alone. Each result is
//!   converted with the conversion [`capture`] applies and judged by calc-verdict:
//!   - B1 spilling over B1:C3: B1 stays in the inventory with no raw observation, saved cache or
//!     reopened value, a spill failure names Sheet1!B1:C3 with 3 rows and 2 columns, and a
//!     calculated B1 equal to the value its Value2 shows (1) is `not_run`, never matched;
//!   - B1 not spilling, though the input saved it over B1:C3: recorded as Excel shows it, flagged
//!     as a dynamic-array formula, with a save failure for the saved range;
//!   - the stand-in saving `worker/spill-saved-cm.xlsx`, whose B1's cell metadata names no
//!     dynamic-array metadata: a save failure only the saved package gives (the input's formulas
//!     give none); a spill over B1:C4 against a saved B1:C3: a save failure; the stand-in saving
//!     `worker/spill-saved-values.xlsx`, with no formula in B1: the worker's save failure names B1;
//!   - a SpillingToRange or HasSpill that cannot be read: the conversion refuses the capture;
//!   - SpecialCells listing the cells B1 spills into: spill members, no inventory failure, and the
//!     record is judged; a listed cell outside the spill: an inventory failure.
//!
//!   What the stand-in part does not show is what Excel itself reports; the excel part shows it.
//! - The excel part, only on the `--platform excel` run: a live capture
//!   of `worker/spill.xlsx` through [`capture`]. It shows what the stand-in cannot: Range.HasSpill
//!   True for B1 and False for D1 and E1; B1's SpillingToRange, read through Get-ComProperty with
//!   arguments, as `Address(False, False)` B1:C3 with Rows.Count 3 and Columns.Count 2 of the
//!   integer type the worker requires (any other type would have made the conversion refuse the
//!   capture), after the rebuild and after reopening; the package Excel saved holding B1, read from
//!   disk, as an array formula over B1:C3 with cell metadata that the worker resolves to XLDAPR;
//!   and whether SpecialCells(xlCellTypeFormulas) lists the cells B1 spills into (reported either
//!   way), none of them an inventory failure, so calc-verdict judges the record.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

#[cfg(windows)]
use calc_oracle_excel::{
    CalculationPolicy, CaptureError, CaptureRequest, ReportedExcel, STAGE_SPILL, Worker, capture,
    excel_available, run_isolated, sha256_hex, windows_powershell,
};
#[cfg(windows)]
use calc_verdict::value::{CalcValue, CoreValue, WorksheetErrorCode};
#[cfg(windows)]
use calc_verdict::{
    CacheCellType, CaptureRecord, CellAddress, CellObservation, CellResult, Evaluation,
    EvaluationRecord, Observation, Provenance, ResultSet, RunIdentity, VerdictPolicy, judge,
};
#[cfg(windows)]
use std::collections::BTreeMap;
#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CASE: &str = "infra_m1_2__untrusted_workbook";

/// The variable the acceptance runner sets to the platform route a case runs on.
#[cfg(windows)]
const PLATFORM_VARIABLE: &str = "DNACALC_ACCEPT_PLATFORM";

#[test]
fn infra_m1_2__untrusted_workbook() {
    #[cfg(not(windows))]
    println!(
        "ACCEPT-NOT-RUN: {CASE}: the isolation of an Excel capture is a Windows mechanism; this host is {}",
        std::env::consts::OS
    );
    #[cfg(windows)]
    {
        windows::isolation_part();
        match std::env::var(PLATFORM_VARIABLE).as_deref() {
            Ok("excel") => windows::excel_part(),
            route => println!(
                "ACCEPT-NOTE: {CASE}: the excel part (live captures of the untrusted fixtures: macros, the user-defined function and the external link) runs only on the excel route; this run's route is {route:?}, so only the isolation part ran and no Excel was started"
            ),
        }
    }
}

const SPILL_CASE: &str = "dc_xb26__spill_shape";

#[test]
fn dc_xb26__spill_shape() {
    #[cfg(not(windows))]
    println!(
        "ACCEPT-NOT-RUN: {SPILL_CASE}: the capture worker runs under Windows PowerShell; this host is {}",
        std::env::consts::OS
    );
    #[cfg(windows)]
    {
        let route = std::env::var(PLATFORM_VARIABLE).unwrap_or_default();
        windows::spill_standin_part(if route.is_empty() { "plain" } else { &route });
        if route == "excel" {
            windows::spill_excel_part();
        } else {
            println!(
                "ACCEPT-NOTE: {SPILL_CASE}: the excel part (a live capture of worker/spill.xlsx: HasSpill, SpillingToRange's address, rows and columns, the saved package's ref and cell metadata, and whether SpecialCells lists the cells of a spill) runs only on the excel route; this run's route is {route:?}, so only the stand-in part ran and no Excel was started"
            );
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::process::CommandExt;

    /// CREATE_NO_WINDOW.
    const NO_WINDOW: u32 = 0x0800_0000;

    /// The image of the stand-in Excel and of the sentinel.
    const STANDIN_IMAGE: &str = "powershell.exe";

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
    }

    /// A token no other process on this host carries in its command line.
    fn token(what: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        format!("dnacalc-infra-m1-2-{what}-{}-{nanos}", std::process::id())
    }

    fn quiet_powershell() -> Command {
        let mut command = Command::new(windows_powershell());
        command
            .args(["-NoLogo", "-NoProfile", "-NonInteractive"])
            .stdin(Stdio::null())
            .creation_flags(NO_WINDOW);
        command
    }

    /// The output of a PowerShell command, which must succeed.
    fn powershell_output(command: &str) -> String {
        let out = quiet_powershell()
            .args(["-Command", command])
            .output()
            .expect("PowerShell starts");
        assert!(
            out.status.success(),
            "PowerShell failed on {command:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// The PIDs of the running processes whose command line holds `token`, read from
    /// Win32_Process (the query's own process left out).
    fn processes_holding(token: &str) -> Vec<u32> {
        powershell_output(&format!(
            "Get-CimInstance -ClassName Win32_Process | Where-Object {{ $_.ProcessId -ne $PID -and $_.CommandLine -and $_.CommandLine.Contains('{token}') }} | ForEach-Object {{ $_.ProcessId }}"
        ))
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
    }

    /// The parent PID Win32_Process records for `pid`.
    fn parent_of(pid: u32) -> Option<u32> {
        powershell_output(&format!(
            "(Get-CimInstance -ClassName Win32_Process -Filter 'ProcessId={pid}').ParentProcessId"
        ))
        .trim()
        .parse()
        .ok()
    }

    /// The stand-in worker. With `report` it reports that PID and starts nothing; otherwise it
    /// starts a stand-in Excel whose command line holds `nonce` and which writes its PID to
    /// `started`. It hangs when `hang`.
    fn standin(
        dir: &Path,
        nonce: &str,
        started: &Path,
        report: Option<u32>,
        hang: bool,
        timeout: Duration,
    ) -> Worker {
        let mut args: Vec<std::ffi::OsString> = [
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ]
        .iter()
        .map(Into::into)
        .collect();
        args.push(fixtures().join("standin-worker.ps1").into_os_string());
        match report {
            Some(pid) => args.extend(["-ReportPid".into(), pid.to_string().into()]),
            None => {
                args.extend(["-Nonce".into(), nonce.into(), "-StartedFile".into()]);
                args.push(started.as_os_str().to_owned());
            }
        }
        if hang {
            args.push("-Hang".into());
        }
        Worker {
            program: windows_powershell(),
            args,
            dir: dir.to_owned(),
            timeout,
            exit_grace: Duration::from_secs(2),
            excel_image: STANDIN_IMAGE.to_owned(),
        }
    }

    fn started_pid(started: &Path) -> u32 {
        std::fs::read_to_string(started)
            .unwrap_or_else(|e| {
                panic!(
                    "the stand-in Excel wrote no PID to {}: {e}",
                    started.display()
                )
            })
            .trim()
            .trim_start_matches('\u{feff}')
            .parse()
            .expect("the stand-in Excel wrote its PID")
    }

    pub(super) fn isolation_part() {
        let dir = std::env::temp_dir().join(token("isolation"));
        std::fs::create_dir_all(&dir).expect("the case's directory");

        // The sentinel: another process of the stand-in Excel's image, started before any run,
        // standing for an Excel the capture did not start.
        let sentinel_token = token("sentinel");
        let mut sentinel = quiet_powershell()
            .args([
                "-Command",
                &format!("$null = '{sentinel_token}'; Start-Sleep -Seconds 600"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the sentinel starts");

        // A worker that reports its Excel and then hangs: the run times out.
        let hang_nonce = token("hang");
        let hang_started = dir.join("hang-started.txt");
        let timeout = Duration::from_secs(20);
        let began = Instant::now();
        let outcome = run_isolated(&standin(
            &dir,
            &hang_nonce,
            &hang_started,
            None,
            true,
            timeout,
        ));
        let elapsed = began.elapsed();
        let report = match outcome {
            Err(CaptureError::TimedOut(report)) => report,
            other => panic!("a hanging worker must time out, got {other:?}"),
        };
        assert!(
            elapsed >= timeout && elapsed < timeout + Duration::from_secs(30),
            "the run returned after {elapsed:?} with a timeout of {timeout:?}"
        );
        let hang_excel = started_pid(&hang_started);
        assert_eq!(
            report.excel_pid,
            Some(hang_excel),
            "the Excel the adapter terminated is the stand-in that recorded its own PID"
        );
        assert_eq!(report.excel, ReportedExcel::Terminated, "{report:?}");
        let survivors = processes_holding(&hang_nonce);
        assert!(
            survivors.is_empty(),
            "after the timeout the worker and its stand-in Excel must be gone; still running: {survivors:?}"
        );
        assert!(
            matches!(sentinel.try_wait(), Ok(None)),
            "the sentinel, another Excel's stand-in, must survive the timeout"
        );
        assert_eq!(processes_holding(&sentinel_token), [sentinel.id()]);
        println!(
            "ACCEPT-NOTE: {CASE}: a hanging stand-in worker (PID {}) timed out after {:.1}s with a {}s timeout; its stand-in Excel (PID {hang_excel}, the PID it wrote itself, started through WMI outside the worker's process tree) and the worker are gone from Win32_Process, and the sentinel (PID {}, the same image) still runs",
            report.worker_pid,
            elapsed.as_secs_f64(),
            timeout.as_secs(),
            sentinel.id()
        );

        // A worker that exits at once and leaves its Excel running: after the grace period the run
        // terminates that Excel, and only it. The stand-in's parent is not the worker, as an
        // Excel's parent is not: killing the worker's process tree would leave it running.
        let exit_nonce = token("exit");
        let exit_started = dir.join("exit-started.txt");
        let worker = standin(
            &dir,
            &exit_nonce,
            &exit_started,
            None,
            false,
            Duration::from_secs(60),
        );
        let run = run_isolated(&worker).expect("a worker that exits completes its run");
        let exit_excel = started_pid(&exit_started);
        assert_eq!(run.excel_pid, Some(exit_excel));
        assert_eq!(run.exit_code, Some(0));
        assert_eq!(
            run.excel,
            ReportedExcel::Terminated,
            "the stand-in Excel outlived the worker and the grace period: {run:?}"
        );
        let survivors = processes_holding(&exit_nonce);
        assert!(
            survivors.is_empty(),
            "the Excel left behind must be terminated; still running: {survivors:?}"
        );
        assert!(matches!(sentinel.try_wait(), Ok(None)));
        println!(
            "ACCEPT-NOTE: {CASE}: a stand-in worker that exited at once left its stand-in Excel (PID {exit_excel}) running; the run terminated it after the grace period, Win32_Process no longer lists it, and the sentinel still runs"
        );

        // The topology the two runs above relied on, measured on a stand-in left running: its
        // parent is not the worker.
        let topology_nonce = token("topology");
        let topology_started = dir.join("topology-started.txt");
        let mut topology_worker = quiet_powershell()
            .args(["-ExecutionPolicy", "Bypass", "-File"])
            .arg(fixtures().join("standin-worker.ps1"))
            .args(["-Nonce", &topology_nonce, "-StartedFile"])
            .arg(&topology_started)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in worker starts");
        let topology_worker_pid = topology_worker.id();
        let _ = topology_worker.wait();
        let topology_excel = started_pid(&topology_started);
        let parent = parent_of(topology_excel);
        let _ = powershell_output(&format!(
            "Stop-Process -Id {topology_excel} -Force -ErrorAction SilentlyContinue"
        ));
        assert!(
            parent.is_some_and(|parent| parent != topology_worker_pid),
            "the stand-in Excel's parent {parent:?} must not be its worker {topology_worker_pid}"
        );
        println!(
            "ACCEPT-NOTE: {CASE}: a stand-in Excel (PID {topology_excel}) has parent PID {} in Win32_Process, not its worker (PID {topology_worker_pid})",
            parent.unwrap_or_default()
        );

        // A worker that reports the sentinel's PID, an Excel running before the run started: the
        // run refuses it and terminates nothing of it.
        let run = run_isolated(&standin(
            &dir,
            "",
            &dir.join("unused.txt"),
            Some(sentinel.id()),
            false,
            Duration::from_secs(60),
        ))
        .expect("a worker that exits completes its run");
        assert_eq!(run.excel_pid, Some(sentinel.id()));
        let ReportedExcel::Refused(refusal) = run.excel.clone() else {
            panic!("the sentinel is not refused: {run:?}");
        };
        assert!(
            refusal.contains("already running before the capture started"),
            "{refusal}"
        );
        std::thread::sleep(Duration::from_secs(3));
        assert!(
            matches!(sentinel.try_wait(), Ok(None)),
            "the sentinel, reported as the worker's Excel, must not be terminated"
        );
        assert_eq!(processes_holding(&sentinel_token), [sentinel.id()]);
        println!(
            "ACCEPT-NOTE: {CASE}: a stand-in worker that reported the sentinel's PID {} (running before the run started) was refused ({refusal}); the sentinel still runs after the grace period",
            sentinel.id()
        );

        let _ = sentinel.kill();
        let _ = sentinel.wait();
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn request(fixture: PathBuf, companions: Vec<PathBuf>, work_dir: PathBuf) -> CaptureRequest {
        CaptureRequest {
            case: CASE.to_owned(),
            fixture,
            companions,
            work_dir,
            timeout: Duration::from_secs(300),
            policy: CalculationPolicy::default(),
            revision: 0,
            operations: Vec::new(),
            repository: None,
        }
    }

    /// The raw value observed at `sheet`!`row`,`column` in `observations`.
    fn raw_at<'a>(
        observations: &'a [CellObservation],
        sheet: &str,
        row: u32,
        column: u32,
    ) -> Option<&'a calc_verdict::value::CalcValue> {
        let cell = CellAddress::new(sheet, row, column)?;
        observations.iter().find_map(|observed| {
            match (&observed.observation, observed.cell == cell) {
                (Observation::ExcelRaw(value), true) => Some(value),
                _ => None,
            }
        })
    }

    /// The bits of the raw number at `sheet`!`row`,`column` in `observations`.
    fn raw_bits(
        observations: &[CellObservation],
        sheet: &str,
        row: u32,
        column: u32,
    ) -> Option<u64> {
        raw_at(observations, sheet, row, column)
            .and_then(|value| value.as_number())
            .map(f64::to_bits)
    }

    /// The saved cache (type, `v` text) at `sheet`!`row`,`column` of the package Excel saved.
    fn saved_at(
        record: &CaptureRecord,
        sheet: &str,
        row: u32,
        column: u32,
    ) -> Option<(CacheCellType, String)> {
        let cell = CellAddress::new(sheet, row, column)?;
        let session = record.session.as_ref()?;
        session
            .saved
            .iter()
            .find_map(|saved| match (&saved.observation, saved.cell == cell) {
                (Observation::ExcelSavedCache(cache), true) => {
                    Some((cache.cell_type(), cache.text().to_owned()))
                }
                _ => None,
            })
    }

    fn reopened(record: &CaptureRecord) -> &[CellObservation] {
        &record
            .session
            .as_ref()
            .expect("a version 2 record")
            .reopened
    }

    fn setting<'a>(record: &'a CaptureRecord, name: &str) -> Option<&'a str> {
        record.environment.as_ref().and_then(|environment| {
            environment
                .settings
                .iter()
                .find(|(setting, _)| setting == name)
                .map(|(_, value)| value.as_str())
        })
    }

    /// Every file under `dir` named `name`.
    fn files_named(dir: &Path, name: &str) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut pending = vec![dir.to_owned()];
        while let Some(next) = pending.pop() {
            for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.file_name().is_some_and(|file| file == name) {
                    found.push(path);
                }
            }
        }
        found
    }

    /// The text of each part of the package at `path` whose name starts with `prefix` and ends
    /// with `.xml` or `.rels`, read with .NET's ZIP reader, as (part name, text with its line
    /// breaks read as spaces).
    fn package_parts(path: &Path, prefix: &str) -> Vec<(String, String)> {
        let listing = powershell_output(&format!(
            "Add-Type -AssemblyName System.IO.Compression.FileSystem; $z = [System.IO.Compression.ZipFile]::OpenRead('{}'); try {{ foreach ($e in $z.Entries) {{ if ($e.FullName.StartsWith('{prefix}') -and ($e.FullName.EndsWith('.xml') -or $e.FullName.EndsWith('.rels'))) {{ $r = New-Object System.IO.StreamReader($e.Open()); [Console]::Out.WriteLine($e.FullName + [char]9 + $r.ReadToEnd().Replace([char]10, ' ').Replace([char]13, ' ')); $r.Dispose() }} }} }} finally {{ $z.Dispose() }}",
            path.display().to_string().replace('\'', "''")
        ));
        listing
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(name, text)| (name.to_owned(), text.to_owned()))
            .collect()
    }

    fn unescape(text: &str) -> String {
        text.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&")
    }

    /// The value of the attribute `name` in the start tag `tag`.
    fn attribute(tag: &str, name: &str) -> Option<String> {
        let needle = format!(" {name}=\"");
        let start = tag.find(&needle)? + needle.len();
        let end = tag[start..].find('"')? + start;
        Some(unescape(&tag[start..end]))
    }

    /// Each element `<name ...>` of `xml` (with no namespace prefix), as its start tag and the
    /// text up to its end tag (empty for an empty element).
    fn elements<'a>(xml: &'a str, name: &str) -> Vec<(&'a str, &'a str)> {
        let open = format!("<{name} ");
        let close = format!("</{name}>");
        let mut found = Vec::new();
        let mut from = 0;
        while let Some(at) = xml[from..].find(&open).map(|i| i + from) {
            let Some(tag_end) = xml[at..].find('>').map(|i| i + at) else {
                break;
            };
            let tag = &xml[at..tag_end];
            if tag.ends_with('/') {
                found.push((tag, ""));
                from = tag_end;
            } else {
                let body_end = xml[tag_end..]
                    .find(&close)
                    .map_or(xml.len(), |i| i + tag_end);
                found.push((tag, &xml[tag_end + 1..body_end]));
                from = body_end;
            }
        }
        found
    }

    /// The cache of each cell of sheet `sheet` in the package at `path`, by A1 reference, as
    /// (`t`, with a cell without one read as `n`, and the `<v>` text): read from the file on disk
    /// through the workbook part, its relationships and the worksheet part, independently of the
    /// record the capture wrote.
    fn caches_on_disk(path: &Path, sheet: &str) -> BTreeMap<String, (String, String)> {
        let (_, xml) = sheet_xml(path, sheet);
        elements(&xml, "c")
            .into_iter()
            .filter_map(|(tag, body)| {
                let start = body.find("<v>")? + 3;
                let end = body[start..].find("</v>")? + start;
                let value = unescape(&body[start..end]);
                Some((
                    attribute(tag, "r")?,
                    (attribute(tag, "t").unwrap_or_else(|| "n".to_owned()), value),
                ))
            })
            .collect()
    }

    /// The record's saved cache at `sheet`!`row`,`column`, as (`t`, `v`) like [`caches_on_disk`].
    fn recorded_cache(
        record: &CaptureRecord,
        sheet: &str,
        row: u32,
        column: u32,
    ) -> Option<(String, String)> {
        saved_at(record, sheet, row, column).map(|(t, v)| {
            let t = match t {
                CacheCellType::Number => "n",
                CacheCellType::Logical => "b",
                CacheCellType::Error => "e",
                CacheCellType::Text => "str",
            };
            (t.to_owned(), v)
        })
    }

    /// The capture recorded no isolation failure, Excel read back the session settings the worker
    /// set, and Excel opened the input read-only. (The worker refuses a capture when an add-in is
    /// open in its instance, so a capture taken shows only that the worker read none as open;
    /// `add_ins`, `com_add_ins_connected` and `add_ins_installed` list what it read.)
    fn assert_isolated(record: &CaptureRecord, what: &str) {
        for (name, expected) in [
            ("automation_security", "3"),
            ("enable_events", "false"),
            ("interactive", "false"),
            ("display_alerts", "false"),
            ("opened_read_only", "true"),
            ("ole_dde_links", ""),
            ("connections", ""),
        ] {
            assert_eq!(setting(record, name), Some(expected), "{what}: {name}");
        }
        let failures = &record
            .session
            .as_ref()
            .expect("a version 2 record")
            .failures;
        assert!(
            failures.iter().all(|failure| failure.stage != "isolation"),
            "{what}: isolation failures {failures:?}"
        );
    }

    pub(super) fn excel_part() {
        let fixtures = fixtures().join("infra_m1_2");
        let macro_fixture = fixtures.join("untrusted-macro.xlsm");
        if !macro_fixture.is_file() {
            println!(
                "ACCEPT-NOT-RUN: {CASE}: the Excel-authored fixture {} is absent; the documentation at the top of platform/calc-oracle-excel/tests/acceptance.rs says how to author it in Excel",
                macro_fixture.display()
            );
            return;
        }
        if let Err(reason) = excel_available() {
            println!("ACCEPT-NOT-RUN: {CASE}: {reason}");
            return;
        }
        assert!(
            files_named(&fixtures, "macro-ran.txt").is_empty(),
            "a macro-ran.txt lies in the fixture directory before the capture; remove it"
        );
        let root = std::env::temp_dir().join(token("excel"));

        let link = capture(&request(
            fixtures.join("untrusted-link.xlsx"),
            vec![fixtures.join("link-target.xlsx")],
            root.join("link"),
        ))
        .unwrap_or_else(|e| panic!("the capture of untrusted-link.xlsx failed: {e}"));
        let record = &link.record;
        assert_isolated(record, "untrusted-link.xlsx");
        assert!(
            setting(record, "external_links")
                .is_some_and(|links| links.contains("link-target.xlsx")),
            "Excel did not report the external link: {:?}",
            setting(record, "external_links")
        );
        // After the full rebuild.
        assert_eq!(
            raw_bits(&record.observations, "Sheet1", 1, 1),
            Some(111f64.to_bits()),
            "Sheet1!A1 must hold the link's cached 111, not the linked file's 333"
        );
        assert_eq!(
            raw_bits(&record.observations, "Sheet1", 1, 2),
            Some(112f64.to_bits())
        );
        // In the package Excel saved: the cells' caches, and the external link part's own cache.
        let output = std::fs::read(&link.output_package).expect("the saved package");
        assert_eq!(
            record.session.as_ref().unwrap().output_package,
            Some(format!("sha256:{}", sha256_hex(&output))),
            "the record names the package on disk"
        );
        // The caches as the saved file holds them, read from disk; the record must say the same.
        let on_disk = caches_on_disk(&link.output_package, "Sheet1");
        for (a1, row, column, expected) in [("A1", 1, 1, "111"), ("B1", 1, 2, "112")] {
            let cache = on_disk.get(a1).cloned();
            assert_eq!(
                cache,
                Some(("n".to_owned(), expected.to_owned())),
                "the saved package's Sheet1!{a1} cache, read from disk"
            );
            assert_eq!(
                recorded_cache(record, "Sheet1", row, column),
                cache,
                "Sheet1!{a1}"
            );
        }
        let link_parts: Vec<(String, String)> =
            package_parts(&link.output_package, "xl/externalLinks/")
                .into_iter()
                .filter(|(name, _)| name.ends_with(".xml"))
                .collect();
        assert!(
            !link_parts.is_empty()
                && link_parts
                    .iter()
                    .all(|(_, text)| text.contains("<v>111</v>") && !text.contains("333")),
            "the saved package's external link parts must still cache 111 and never 333: {link_parts:?}"
        );
        // After reopening the saved package.
        assert_eq!(
            raw_bits(reopened(record), "Sheet1", 1, 1),
            Some(111f64.to_bits()),
            "after reopening, Sheet1!A1 must still hold 111"
        );
        assert_eq!(
            raw_bits(reopened(record), "Sheet1", 1, 2),
            Some(112f64.to_bits())
        );
        println!(
            "ACCEPT-NOTE: {CASE}: untrusted-link.xlsx captured by Excel {} with its external link to link-target.xlsx present and never refreshed: A1 = 111 (the cache) and B1 = 112 after the full rebuild, in the saved package's caches and external link part ({} part(s)) read from disk, and after reopening, while link-target.xlsx holds 333",
            record
                .environment
                .as_ref()
                .map_or("?", |environment| environment.target.excel_build()),
            link_parts.len()
        );
        for name in ["add_ins", "com_add_ins_connected", "add_ins_installed"] {
            let value = setting(record, name)
                .unwrap_or_else(|| panic!("the record carries no setting {name}"));
            assert!(
                !value.contains('\\') && !value.contains('/'),
                "the setting {name} holds a path: {value:?}"
            );
        }
        let connected = setting(record, "com_add_ins_connected").unwrap_or_default();
        println!(
            "ACCEPT-NOTE: {CASE}: the add-ins the worker read (none open in its instance, or the capture would have been refused), by name only: add_ins {:?}; {} connected COM add-ins, recorded, left connected and counted by the verdict: {connected:?}; installed add-ins: {:?}",
            setting(record, "add_ins"),
            connected.split(';').filter(|item| !item.is_empty()).count(),
            setting(record, "add_ins_installed")
        );

        // The positive control: as authored, the fixture caches 7 for C1, what MarkUdf returns
        // when VBA runs, read from the committed file on disk.
        let authored = caches_on_disk(&macro_fixture, "Sheet1");
        assert_eq!(
            authored.get("C1"),
            Some(&("n".to_owned(), "7".to_owned())),
            "the fixture as authored must cache MarkUdf's 7 in Sheet1!C1"
        );
        let macros = capture(&request(macro_fixture, Vec::new(), root.join("macro")))
            .unwrap_or_else(|e| panic!("the capture of untrusted-macro.xlsm failed: {e}"));
        let record = &macros.record;
        assert_isolated(record, "untrusted-macro.xlsm");
        assert_eq!(
            setting(record, "has_vb_project"),
            Some("true"),
            "the macro fixture must hold a VBA project for its macros' silence to mean anything"
        );
        for (observed, when) in [
            (&record.observations[..], "after the full rebuild"),
            (reopened(record), "after reopening the saved package"),
        ] {
            assert_eq!(
                raw_bits(observed, "Sheet1", 1, 2),
                Some(2f64.to_bits()),
                "Sheet1!B1 (=A1*2) must be 2 {when}: a macro that ran would have set A1 to 100"
            );
            let udf = raw_at(observed, "Sheet1", 1, 3);
            assert!(
                udf.is_some_and(|value| value.core()
                    == &CoreValue::Error(WorksheetErrorCode::Name)),
                "Sheet1!C1 (=MarkUdf()) must be #NAME? {when}, not the 7 MarkUdf returns when VBA runs: {udf:?}"
            );
        }
        // The saved file on disk: B1 caches 2 and C1 an error, as the record says.
        let on_disk = caches_on_disk(&macros.output_package, "Sheet1");
        assert_eq!(
            on_disk.get("B1"),
            Some(&("n".to_owned(), "2".to_owned())),
            "the saved package's Sheet1!B1 cache, read from disk"
        );
        assert_eq!(
            on_disk.get("C1"),
            Some(&("e".to_owned(), "#NAME?".to_owned())),
            "the saved package's Sheet1!C1 cache, read from disk"
        );
        for (a1, column) in [("B1", 2), ("C1", 3)] {
            assert_eq!(
                recorded_cache(record, "Sheet1", 1, column).as_ref(),
                on_disk.get(a1),
                "Sheet1!{a1}"
            );
        }
        let markers: Vec<PathBuf> = files_named(&root, "macro-ran.txt")
            .into_iter()
            .chain(files_named(&fixtures, "macro-ran.txt"))
            .collect();
        assert!(markers.is_empty(), "a macro ran and wrote {markers:?}");
        println!(
            "ACCEPT-NOTE: {CASE}: untrusted-macro.xlsm (with a VBA project; as authored it caches C1 = 7, MarkUdf's value) captured, saved and reopened with B1 = 2 and C1 (=MarkUdf()) = {:?} through the full rebuild, and no macro-ran.txt in the work directory or beside the fixture: neither Workbook_Open, Auto_Open nor the user-defined function ran",
            raw_at(&record.observations, "Sheet1", 1, 3)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The unit tests that hold the spill scenarios, by their exact names: the worker's main flow
    /// against the stand-in Excel, converted and judged (`src/worker_tests.rs`), the fixtures they
    /// read as their generator's output, and the conversion's spill rules (`src/convert.rs`). They
    /// are unit tests because the conversion is crate-private: the record it gives is stamped a
    /// live capture.
    const SPILL_UNIT_TESTS: [&str; 13] = [
        "worker_tests::a_formula_that_spills_is_recorded_as_a_spill_never_as_a_scalar",
        "worker_tests::a_dynamic_array_formula_that_does_not_spill_is_recorded_as_excel_shows_it",
        "worker_tests::a_saved_package_that_disagrees_with_the_spill_is_a_save_failure",
        "worker_tests::a_spilling_cell_without_a_saved_formula_is_a_save_failure_naming_it",
        "worker_tests::a_spill_that_cannot_be_read_refuses_the_capture",
        "worker_tests::cells_excel_lists_in_a_spill_range_are_spill_members_not_inventory_failures",
        "worker_tests::fixtures_are_the_output_of_their_generator",
        "convert::tests::a_cell_that_spills_is_recorded_as_a_spill_never_as_a_scalar",
        "convert::tests::a_spill_whose_shape_cannot_be_read_refuses_the_capture",
        "convert::tests::a_saved_ref_that_disagrees_with_the_spill_is_a_save_failure",
        "convert::tests::a_dynamic_array_input_that_no_longer_spills_is_recorded_as_excel_shows_it",
        "convert::tests::a_spill_that_changes_after_reopening_is_a_reopen_failure",
        "convert::tests::spill_members_must_lie_in_a_recorded_spill",
    ];

    /// The stand-in part: runs [`SPILL_UNIT_TESTS`] by their exact names with cargo, in a target
    /// directory of its own (the outer cargo owns its own), keeps the transcript under
    /// `target/accept/dc-xb26/spill-shape-unit-<route>/`, and requires each to have run and passed.
    /// The notes the tests print (`NOTE: ...`) are passed on as this case's notes; the inner
    /// transcript's own lines are passed on only when the run fails, each prefixed, so this case's
    /// transcript keeps one libtest result line.
    pub(super) fn spill_standin_part(route: &str) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let evidence = root
            .join("target")
            .join("accept")
            .join("dc-xb26")
            .join(format!("spill-shape-unit-{route}"));
        std::fs::create_dir_all(&evidence).expect("the evidence directory");
        // The cargo running this case (the CARGO variable), else cargo from PATH.
        let (cargo, which) = match std::env::var_os("CARGO") {
            Some(cargo) => (cargo, "the CARGO variable's cargo"),
            None => ("cargo".into(), "cargo from PATH (CARGO is unset)"),
        };
        let args: Vec<&str> = ["test", "-p", "calc-oracle-excel", "--lib", "--locked", "--"]
            .into_iter()
            .chain(SPILL_UNIT_TESTS)
            .chain(["--exact", "--show-output"])
            .collect();
        let mut command = Command::new(&cargo);
        command
            .args(&args)
            .current_dir(&root)
            .env_remove("CARGO_BUILD_TARGET_DIR")
            .env(
                "CARGO_TARGET_DIR",
                root.join("target").join("calc-oracle-excel-unit"),
            )
            .stdin(Stdio::null())
            .creation_flags(NO_WINDOW);
        let shown = format!(
            "cargo {} ({which}, CARGO_TARGET_DIR target/calc-oracle-excel-unit)",
            args.join(" ")
        );
        let out = command.output().expect("cargo starts");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        std::fs::write(evidence.join("stdout.txt"), &stdout).expect("the transcript");
        std::fs::write(evidence.join("stderr.txt"), &stderr).expect("the transcript");
        std::fs::write(
            evidence.join("exit.txt"),
            format!("{shown}\nexit code: {:?}\n", out.status.code()),
        )
        .expect("the transcript");
        let prefixed =
            |text: &str| -> String { text.lines().map(|line| format!("unit| {line}\n")).collect() };
        let results: Vec<&str> = stdout
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("test result: "))
            .collect();
        let expected = format!(
            "test result: ok. {} passed; 0 failed; 0 ignored;",
            SPILL_UNIT_TESTS.len()
        );
        let missing: Vec<&str> = SPILL_UNIT_TESTS
            .iter()
            .copied()
            .filter(|name| {
                !stdout
                    .lines()
                    .any(|line| line == format!("test {name} ... ok"))
            })
            .collect();
        assert!(
            out.status.success()
                && results.len() == 1
                && results[0].starts_with(&expected)
                && missing.is_empty(),
            "the spill unit tests did not all run and pass (exit {:?}; result lines {results:?}; not ok: {missing:?}); {shown}\n{}{}",
            out.status.code(),
            prefixed(&stdout),
            prefixed(&stderr)
        );
        for line in stdout.lines() {
            if let Some(note) = line.strip_prefix("NOTE: ") {
                println!("ACCEPT-NOTE: {SPILL_CASE}: {note}");
            }
        }
        println!(
            "ACCEPT-NOTE: {SPILL_CASE}: the stand-in part ran the {} spill unit tests of src/worker_tests.rs and src/convert.rs by exact name ({}), each ok; transcript and exit code in {}",
            SPILL_UNIT_TESTS.len(),
            results[0],
            evidence.strip_prefix(&root).unwrap_or(&evidence).display()
        );
    }

    /// The cells of Sheet1, row 1, of `worker/spill.xlsx`: B1 =SEQUENCE(3,2), D1 =A1+1 and E1
    /// =SUM(A1:A2*2).
    const B1: u64 = 2;
    const D1: u64 = 4;
    const E1: u64 = 5;

    /// The part of `sheet` of the package at `path` (found through the workbook part and its
    /// relationships, as [`caches_on_disk`] finds it), with every `xl/` part by name.
    fn sheet_xml(path: &Path, sheet: &str) -> (BTreeMap<String, String>, String) {
        let parts: BTreeMap<String, String> = package_parts(path, "xl/").into_iter().collect();
        let workbook = parts
            .get("xl/workbook.xml")
            .expect("the saved workbook part");
        let rels = parts
            .get("xl/_rels/workbook.xml.rels")
            .expect("the saved workbook's relationships");
        let id = elements(workbook, "sheet")
            .into_iter()
            .find(|(tag, _)| attribute(tag, "name").as_deref() == Some(sheet))
            .and_then(|(tag, _)| attribute(tag, "r:id"))
            .unwrap_or_else(|| panic!("the saved workbook has no sheet {sheet}"));
        let target = elements(rels, "Relationship")
            .into_iter()
            .find(|(tag, _)| attribute(tag, "Id").as_deref() == Some(id.as_str()))
            .and_then(|(tag, _)| attribute(tag, "Target"))
            .unwrap_or_else(|| panic!("the saved workbook has no relationship {id}"));
        let part = match target.strip_prefix('/') {
            Some(absolute) => absolute.to_owned(),
            None => format!("xl/{target}"),
        };
        let xml = parts
            .get(&part)
            .unwrap_or_else(|| panic!("the saved package has no part {part}"))
            .clone();
        (parts, xml)
    }

    /// The excel part, only on the excel route: a live capture of `worker/spill.xlsx`
    /// through [`capture`], the route `cargo xtask capture` takes, showing what the stand-in
    /// cannot. Excel's own reports are read from the worker's result file in the capture's work
    /// directory, and the saved package from the file on disk.
    pub(super) fn spill_excel_part() {
        if let Err(reason) = excel_available() {
            println!("ACCEPT-NOT-RUN: {SPILL_CASE}: {reason}");
            return;
        }
        let root = std::env::temp_dir().join(token("spill-excel"));
        let work = root.join("capture");
        let outcome = capture(&CaptureRequest {
            case: SPILL_CASE.to_owned(),
            fixture: fixtures().join("worker").join("spill.xlsx"),
            companions: Vec::new(),
            work_dir: work.clone(),
            timeout: Duration::from_secs(300),
            policy: CalculationPolicy::default(),
            revision: 0,
            operations: Vec::new(),
            repository: None,
        })
        .unwrap_or_else(|e| panic!("the capture of worker/spill.xlsx failed: {e}"));
        let record = &outcome.record;
        assert_isolated(record, "spill.xlsx");
        let result: serde_json::Value = serde_json::from_slice(
            &std::fs::read(work.join("result.json")).expect("the worker's result file"),
        )
        .expect("the worker's result is JSON");
        let at = |list: &str, column: u64| -> serde_json::Value {
            result[list]
                .as_array()
                .and_then(|entries| {
                    entries.iter().find(|entry| {
                        entry["sheet"] == "Sheet1" && entry["row"] == 1 && entry["column"] == column
                    })
                })
                .cloned()
                .unwrap_or_else(|| {
                    panic!("the result's {list} holds no Sheet1 row 1 column {column}")
                })
        };

        // 1-3. Range.HasSpill is True for B1 and False for D1 (ordinary) and E1 (a dynamic-array
        // formula over its own cell); B1's Range.SpillingToRange, read through Get-ComProperty
        // (Address with the arguments False, False; Rows.Count; Columns.Count), is B1:C3 with 3
        // rows and 2 columns. The worker writes a spill as unread when HasSpill is not a boolean,
        // the address not a text or a count not an integer, and the conversion then refuses the
        // capture, so a capture taken shows each read of the type the worker expects.
        for list in ["cells", "reopened"] {
            assert_eq!(
                at(list, B1)["spill"],
                serde_json::json!({"ref": "B1:C3", "rows": 3, "columns": 2}),
                "Sheet1!B1 in the result's {list}"
            );
            for column in [D1, E1] {
                assert_eq!(
                    at(list, column)["spill"],
                    serde_json::Value::Null,
                    "Sheet1 row 1 column {column} in the result's {list}"
                );
            }
        }
        let b1 = CellAddress::new("Sheet1", 1, 2).unwrap();
        let b1_failures: Vec<(&str, &str)> = record
            .session
            .as_ref()
            .expect("a version 2 record")
            .failures
            .iter()
            .filter(|failure| failure.cell.as_ref() == Some(&b1))
            .map(|failure| (failure.stage.as_str(), failure.message.as_str()))
            .collect();
        assert_eq!(
            b1_failures,
            [(
                STAGE_SPILL,
                "Excel reports a spill over Sheet1!B1:C3 (3 rows by 2 columns) after the full rebuild; CaptureRecord version 2 holds no array, so the cell's value, saved cache and reopened value are not recorded and it is not compared as a scalar"
            )],
            "the failures of Sheet1!B1 (a save failure here would be the saved package disagreeing)"
        );
        assert_eq!(raw_at(&record.observations, "Sheet1", 1, 2), None);
        assert_eq!(
            raw_bits(&record.observations, "Sheet1", 1, 4),
            Some(2f64.to_bits())
        );
        assert_eq!(
            raw_bits(&record.observations, "Sheet1", 1, 5),
            Some(6f64.to_bits())
        );
        println!(
            "ACCEPT-NOTE: {SPILL_CASE}: Excel {} reports Sheet1!B1 (=SEQUENCE(3,2)) with HasSpill True and SpillingToRange B1:C3 (Address(False, False)), Rows.Count 3 and Columns.Count 2 as integers, after the full rebuild and after reopening; D1 and E1 with HasSpill False; the record has B1's spill failure and no observation for it, and D1 = 2, E1 = 6",
            record
                .environment
                .as_ref()
                .map_or("?", |environment| environment.target.excel_build())
        );

        // 4. The saved package's B1, read from the file on disk: an array formula over B1:C3 whose
        // cell metadata the worker resolves to the dynamic-array metadata XLDAPR.
        let (parts, xml) = sheet_xml(&outcome.output_package, "Sheet1");
        let (tag, body) = elements(&xml, "c")
            .into_iter()
            .find(|(tag, _)| attribute(tag, "r").as_deref() == Some("B1"))
            .expect("the saved package holds Sheet1!B1");
        let cm = attribute(tag, "cm");
        let formula = elements(body, "f").into_iter().next().map(|(f, _)| f);
        assert!(
            cm.is_some()
                && formula.is_some_and(|f| {
                    attribute(f, "t").as_deref() == Some("array")
                        && attribute(f, "ref").as_deref() == Some("B1:C3")
                }),
            "the saved package's Sheet1!B1, read from disk: {tag}>{body}"
        );
        let metadata = parts
            .iter()
            .find(|(name, _)| name.ends_with("metadata.xml"))
            .map(|(_, text)| text.as_str())
            .unwrap_or_default();
        assert!(
            metadata.contains("\"XLDAPR\""),
            "the saved package's metadata part names no XLDAPR: {metadata}"
        );
        assert_eq!(
            at("saved_formulas", B1)["dynamic_array_metadata"],
            serde_json::json!(true),
            "the worker's reading of the saved B1's cell metadata"
        );
        println!(
            "ACCEPT-NOTE: {SPILL_CASE}: the package Excel saved holds Sheet1!B1 as {} with cell metadata cm={}, which the worker resolves to the XLDAPR dynamic-array metadata; no save failure",
            formula.unwrap_or_default(),
            cm.unwrap_or_default()
        );

        // 5. Whether SpecialCells(xlCellTypeFormulas) lists the cells B1 spills into: either way,
        // each it lists is a spill member inside B1:C3, never an inventory failure, and
        // calc-verdict judges the record.
        let members: Vec<(u64, u64)> = result["spill_members"]
            .as_array()
            .expect("the result's spill members")
            .iter()
            .map(|member| {
                assert_eq!(member["sheet"], "Sheet1", "{member}");
                (
                    member["row"].as_u64().unwrap(),
                    member["column"].as_u64().unwrap(),
                )
            })
            .collect();
        assert!(
            members.iter().all(|&(row, column)| (1..=3).contains(&row)
                && (2..=3).contains(&column)
                && (row, column) != (1, 2)),
            "spill members outside B1:C3: {members:?}"
        );
        let inventory: Vec<&str> = record
            .session
            .as_ref()
            .unwrap()
            .failures
            .iter()
            .filter(|failure| failure.stage == "inventory")
            .map(|failure| failure.message.as_str())
            .collect();
        assert!(inventory.is_empty(), "inventory failures: {inventory:?}");
        let judged =
            judged(record).unwrap_or_else(|e| panic!("calc-verdict refuses the record: {e}"));
        assert_eq!(judged[0], ("B1".to_owned(), "not_run"), "{judged:?}");
        println!(
            "ACCEPT-NOTE: {SPILL_CASE}: SpecialCells(xlCellTypeFormulas) listed {} of the cells B1 spills into ({members:?}, as (row, column)), each recorded as a spill member and none as an inventory failure; calc-verdict judges the record: {judged:?}",
            if members.is_empty() { "none" } else { "some" }
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The verdicts calc-verdict gives `record`'s B1, D1 and E1 when a run calculated B1 as 1 (the
    /// top-left value of its array), D1 as 2 and E1 as 6, with an evaluation of each, or why it
    /// refuses to judge the record.
    fn judged(record: &CaptureRecord) -> Result<Vec<(String, &'static str)>, String> {
        let session = record.session.as_ref().expect("a version 2 record");
        let cells = [(B1, 1.0), (D1, 2.0), (E1, 6.0)];
        let cell = |column: u64| CellAddress::new("Sheet1", 1, column as u32).unwrap();
        let results = ResultSet {
            case: record.case.clone(),
            source: record.source.clone(),
            run: RunIdentity {
                revision: session.revision,
                context: session.context.clone(),
            },
            results: cells
                .iter()
                .map(|(column, value)| CellResult {
                    cell: cell(*column),
                    provenance: Provenance::Calculated {
                        tick: 1,
                        revision: session.revision,
                        context: session.context.clone(),
                    },
                    value: Some(CalcValue::number(*value)),
                })
                .collect(),
        };
        let evaluations = EvaluationRecord {
            case: record.case.clone(),
            source: record.source.clone(),
            evaluations: cells
                .iter()
                .map(|(column, _)| Evaluation {
                    cell: cell(*column),
                    tick: 1,
                    revision: session.revision,
                    context: session.context.clone(),
                })
                .collect(),
        };
        let report = judge(record, &results, &evaluations, &VerdictPolicy::default())
            .map_err(|e| e.to_string())?;
        Ok(report
            .cells
            .iter()
            .map(|cell| (cell.formula.cell.a1(), cell.verdict.name()))
            .collect())
    }
}
