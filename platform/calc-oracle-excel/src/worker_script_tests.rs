//! Text-only checks of the production capture worker. These also run on Linux.

use crate::WORKER_SCRIPT;

// --- text checks of the script ------------------------------------------------------------------

/// The trimmed line on which each catch with an empty body starts: `catch {}`, `catch { }`, a
/// body of white space or comments over several lines, with or without exception types.
fn empty_catches(script: &str) -> Vec<String> {
    let bytes = script.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = script[from..].find("catch").map(|i| i + from) {
        from = at + 5;
        let word_start =
            at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        let word_end = bytes
            .get(at + 5)
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_'));
        if !word_start || !word_end {
            continue;
        }
        let mut i = at + 5;
        // Exception types: [Type], [Type], ...
        loop {
            while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
                i += 1;
            }
            match (bytes.get(i), script[i..].find(']')) {
                (Some(b'['), Some(end)) => i += end + 1,
                _ => break,
            }
        }
        if bytes.get(i) != Some(&b'{') {
            continue;
        }
        i += 1;
        // White space and comments only, up to the closing brace.
        loop {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if script[i..].starts_with("<#") {
                i = script[i..]
                    .find("#>")
                    .map_or(bytes.len(), |end| i + end + 2);
            } else if bytes.get(i) == Some(&b'#') {
                i = script[i..].find('\n').map_or(bytes.len(), |end| i + end);
            } else {
                break;
            }
        }
        if bytes.get(i) == Some(&b'}') {
            let start = script[..at].rfind('\n').map_or(0, |n| n + 1);
            let end = script[at..].find('\n').map_or(script.len(), |n| at + n);
            found.push(script[start..end].trim().to_owned());
        }
    }
    found
}

/// The lint finds every form of an empty catch, and no catch that does something.
#[test]
fn the_empty_catch_lint_finds_every_form() {
    let sample = "try { a } catch {}\n\
                  try { b } catch { }\n\
                  try { c } catch {\n    \n}\n\
                  try { d } catch [System.IO.IOException], [System.Exception] { }\n\
                  try { e } catch { # nothing to do\n}\n\
                  try { f } catch {<# nothing #>}\n\
                  try { g } catch { Add-Failure 'x' $null 'y' }\n\
                  # every catch below records a failure\n\
                  $catches = 1; try { h } catch{throw}\n";
    assert_eq!(
        empty_catches(sample),
        [
            "try { a } catch {}",
            "try { b } catch { }",
            "try { c } catch {",
            "try { d } catch [System.IO.IOException], [System.Exception] { }",
            "try { e } catch { # nothing to do",
            "try { f } catch {<# nothing #>}",
        ]
    );
}

/// Every catch of the worker records a failure or refuses the capture, except the cleanup of its
/// finally block: an empty catch appears only on the four cleanup lines, including restoration
/// of the capture's requested macro policy.
#[test]
fn the_worker_swallows_no_error_outside_its_cleanup() {
    assert_eq!(
        empty_catches(WORKER_SCRIPT),
        [
            "if ($null -ne $workbook) { try { $workbook.Close($false) } catch { } }",
            "if ($null -ne $excel -and $null -ne $securityBefore -and $wantedSecurity -ne $securityBefore) { try { $excel.AutomationSecurity = $securityBefore } catch { } }",
            "try { $excel.Quit() } catch { }",
            "try { [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($excel) } catch { }",
        ]
    );
}

/// The members `var.member = ...` assigns in `script`, as "var.member" (`var` without its `$`),
/// for a plain or a dynamic member name; `==` is not an assignment.
pub(super) fn member_assignments(script: &str) -> Vec<String> {
    let bytes = script.as_bytes();
    let name = |at: usize| {
        let end = script[at..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
            .map_or(script.len(), |n| at + n);
        (end, &script[at..end])
    };
    let mut found = Vec::new();
    for (at, _) in script.match_indices('$') {
        let (dot, var) = name(at + 1);
        if var.is_empty() || bytes.get(dot) != Some(&b'.') {
            continue;
        }
        let (end, member) = name(dot + 1);
        let mut i = end;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if !member.is_empty() && bytes.get(i) == Some(&b'=') && bytes.get(i + 1) != Some(&b'=') {
            found.push(format!("{var}.{member}"));
        }
    }
    found
}

/// The worker changes no setting that persists beyond its Excel instance: the only members of
/// Excel's Application it assigns are the session settings (including the requested macro
/// policy and its restoration) and the calculation settings, it
/// assigns no AskToUpdateLinks, IgnoreRemoteRequests, Connect, Installed or FeatureInstall on any
/// object and no member by a computed name, and it writes no registry value.
#[test]
fn the_worker_assigns_only_session_settings() {
    let assigned = member_assignments(WORKER_SCRIPT);
    let mut excel: Vec<&str> = assigned
        .iter()
        .filter_map(|a| a.strip_prefix("excel."))
        .collect();
    excel.sort_unstable();
    // Calculation is assigned in two places: before a manual capture opens the workbook, and
    // after any other opens it (dc-10y).
    excel.dedup();
    assert_eq!(
        excel,
        [
            "AutomationSecurity",
            "Calculation",
            "DisplayAlerts",
            "EnableEvents",
            "Interactive",
            "Iteration",
            "MaxChange",
            "MaxIterations",
            "ScreenUpdating",
            "Visible",
        ]
    );
    // Macro policy may only be assigned on this capture's Application, from the request
    // or the value held before opening it. Keep other objects and arbitrary values forbidden.
    let security: Vec<_> = assigned
        .iter()
        .filter(|a| a.ends_with(".AutomationSecurity"))
        .map(String::as_str)
        .collect();
    assert_eq!(security, ["excel.AutomationSecurity"; 3]);
    let writes: Vec<_> = WORKER_SCRIPT
        .lines()
        .filter(|line| line.contains("$excel.AutomationSecurity ="))
        .map(str::trim)
        .collect();
    assert_eq!(
        writes,
        [
            "$excel.AutomationSecurity = $wantedSecurity",
            "if ($wantedSecurity -ne $securityBefore) { $excel.AutomationSecurity = $securityBefore }",
            "if ($null -ne $excel -and $null -ne $securityBefore -and $wantedSecurity -ne $securityBefore) { try { $excel.AutomationSecurity = $securityBefore } catch { } }",
        ]
    );
    for forbidden in [
        "AskToUpdateLinks",
        "IgnoreRemoteRequests",
        "Connect",
        "Installed",
        "FeatureInstall",
    ] {
        let hits: Vec<&String> = assigned
            .iter()
            .filter(|a| a.ends_with(&format!(".{forbidden}")))
            .collect();
        assert!(hits.is_empty(), "{forbidden} is assigned: {hits:?}");
    }
    let computed: Vec<&String> = assigned.iter().filter(|a| a.contains(".$")).collect();
    assert!(
        computed.is_empty(),
        "members assigned by a computed name: {computed:?}"
    );
    for writer in [
        "Set-ItemProperty",
        "New-ItemProperty",
        "Remove-ItemProperty",
        "reg add",
        "reg.exe",
    ] {
        assert!(!WORKER_SCRIPT.contains(writer), "the worker uses {writer}");
    }
    // The check reads assignments as they are written.
    assert_eq!(
        member_assignments(
            "$excel.AskToUpdateLinks = $false\n$a.Installed= $true\n$excel.$name = 1\nif ($x.Connect == 1) {}"
        ),
        ["excel.AskToUpdateLinks", "a.Installed", "excel.$name"]
    );
}

/// The worker creates its Excel through `New-Object -ComObject` and through nothing else, so the
/// main-flow tests' guard on New-Object stands between them and Excel.
#[test]
fn the_worker_creates_com_objects_only_through_new_object() {
    assert_eq!(WORKER_SCRIPT.matches("-ComObject").count(), 1);
    assert!(WORKER_SCRIPT.contains("$excel = New-Object -ComObject Excel.Application"));
    for other in [
        "GetTypeFromProgID",
        "GetTypeFromCLSID",
        "CreateInstance",
        "GetActiveObject",
        "BindToMoniker",
        "CreateObject",
        "Start-Process",
        "Invoke-Item",
    ] {
        assert!(!WORKER_SCRIPT.contains(other), "the worker uses {other}");
    }
}
