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
//!   is the excel part, the owner's run.
//! - The excel part, only on the owner's `--platform excel` run (the runner sets
//!   `DNACALC_ACCEPT_PLATFORM=excel`; no agent runs it): live captures of the untrusted fixtures
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
//! The excel part needs an Excel profile with no loaded add-ins: the worker reads every add-in's
//! state, switches none off, and refuses the capture, naming each, when one is open, installed or
//! connected (or its state cannot be read). A refusal of this kind on the owner's run lists the
//! add-ins to unload in Excel's add-in dialogs before running again. That refusal itself is
//! exercised without Excel by the worker unit tests, with a stand-in Application.
//!
//! The macro workbook cannot be authored honestly without Excel, so the owner authors it once
//! (this is the handover text the case names when the file is absent). In Excel:
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

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

#[cfg(windows)]
use calc_oracle_excel::{
    CalculationPolicy, CaptureError, CaptureRequest, ReportedExcel, Worker, capture,
    excel_available, run_isolated, sha256_hex, windows_powershell,
};
#[cfg(windows)]
use calc_verdict::value::CoreValue;
#[cfg(windows)]
use calc_verdict::{CacheCellType, CaptureRecord, CellAddress, CellObservation, Observation};
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
                "ACCEPT-NOTE: {CASE}: the excel part (live captures of the untrusted fixtures: macros, the user-defined function and the external link) runs only on the owner's excel route; this run's route is {route:?}, so only the isolation part ran and no Excel was started"
            ),
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
            .unwrap_or_else(|| panic!("the saved package has no part {part}"));
        elements(xml, "c")
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

    /// The capture recorded no isolation failure, and Excel read back the session settings the
    /// worker set. (The worker refuses a capture when an add-in is loaded, so a capture taken
    /// shows only that the worker read none as loaded; `add_ins` lists what it read.)
    fn assert_isolated(record: &CaptureRecord, what: &str) {
        for (name, expected) in [
            ("automation_security", "3"),
            ("enable_events", "false"),
            ("interactive", "false"),
            ("display_alerts", "false"),
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
                "ACCEPT-NOT-RUN: {CASE}: the owner-authored fixture {} is absent; the documentation at the top of platform/calc-oracle-excel/tests/acceptance.rs says how to author it in Excel",
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
        println!(
            "ACCEPT-NOTE: {CASE}: the add-ins the worker read (none loaded, or the capture would have been refused): {:?}",
            setting(record, "add_ins")
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
                udf.is_some_and(|value| matches!(value.core(), CoreValue::Error(_))),
                "Sheet1!C1 (=MarkUdf()) must be a worksheet error {when}, not the 7 MarkUdf returns when VBA runs: {udf:?}"
            );
        }
        // The saved file on disk: B1 caches 2 and C1 an error, as the record says.
        let on_disk = caches_on_disk(&macros.output_package, "Sheet1");
        assert_eq!(
            on_disk.get("B1"),
            Some(&("n".to_owned(), "2".to_owned())),
            "the saved package's Sheet1!B1 cache, read from disk"
        );
        assert!(
            on_disk.get("C1").is_some_and(|(t, _)| t == "e"),
            "the saved package's Sheet1!C1 cache, read from disk, must be an error: {:?}",
            on_disk.get("C1")
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
            "ACCEPT-NOTE: {CASE}: untrusted-macro.xlsm (with a VBA project) captured, saved and reopened with B1 = 2 and C1 (=MarkUdf()) = {:?} through the full rebuild, and no macro-ran.txt in the work directory or beside the fixture: neither Workbook_Open, Auto_Open nor the user-defined function ran",
            raw_at(&record.observations, "Sheet1", 1, 3)
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
