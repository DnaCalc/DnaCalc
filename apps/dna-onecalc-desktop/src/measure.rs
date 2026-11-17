//! `dna-onecalc-desktop --measure-surrogate [--xll PATH] [--rounds N]`: what the add-in process
//! costs against the in-process mode (docs/architecture/ADDIN_SURROGATE.md section 11: measured and
//! reported, not targeted). Prints one JSON object:
//! - `startup_ms`: in-process, the session and the add-in set built; surrogate, from the launch to
//!   the process's hello (it builds the same two);
//! - `call_us`: one page request (`Evaluate` of `=1+1`, or of `=MyAdd(1,2)` with `--xll`), the
//!   median and the slowest of `rounds`;
//! - `callback_us`: one request whose answer needs a service of the window (`Recent`, which in the
//!   surrogate crosses back to the window under the open request as parent).
//!
//! Both modes run the same `Host` code; only the transport differs.

use crate::app::{Addins, session_and_addins};
use crate::host::{Files, Host};
use crate::surrogate::Remote;
use calc_platform_desktop::surrogate::Supervisor;
use calc_platform_desktop::{Shell, pump_pending};
use dna_onecalc::port::{Reply, Request};
use dna_onecalc::shown::Authored;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The window's dialogs, for a run without a person: nothing is chosen.
struct NoDialogs;

impl Files for NoDialogs {
    fn choose_open(&mut self) -> Result<Option<(String, Vec<u8>)>, String> {
        Ok(None)
    }
    fn choose_save(&mut self, _suggested: &str, _bytes: &[u8]) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn choose_addin(&mut self) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
    fn recent(&self) -> Vec<String> {
        vec!["C:\\measure\\recent.xlsx".to_owned()]
    }
}

fn spread(mut samples: Vec<Duration>) -> serde_json::Value {
    samples.sort();
    let median = samples[samples.len() / 2];
    let slowest = samples[samples.len() - 1];
    serde_json::json!({
        "median": median.as_secs_f64() * 1e6,
        "slowest": slowest.as_secs_f64() * 1e6,
        "rounds": samples.len(),
    })
}

fn evaluate(formula: &str) -> Request {
    let mut authored = Authored::default();
    authored.formula = formula.to_owned();
    Request::Evaluate(authored)
}

fn shown(reply: &Reply) -> Result<String, String> {
    match reply {
        Reply::Shown(shown) => Ok(shown.result.clone()),
        other => Err(format!("unexpected answer: {other:?}")),
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let (mut xll, mut rounds) = (None, 200usize);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().cloned().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--xll" => xll = Some(PathBuf::from(value()?)),
            "--rounds" => rounds = value()?.parse().map_err(|_| "--rounds needs a number")?,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let rounds = rounds.max(1);
    let formula = if xll.is_some() { "=MyAdd(1,2)" } else { "=1+1" };
    let load = xll.as_ref().map(|path| Request::LoadAddin {
        path: path.display().to_string(),
        expected_hash: None,
    });

    // In-process.
    let started = Instant::now();
    let shell = Shell::new("DNA OneCalc")?;
    let (session, set) = session_and_addins(&shell, "Sheet1")?;
    let mut host = Host::new(session, NoDialogs, Addins::new(set.clone()));
    let local_startup = started.elapsed();
    if let Some(load) = &load {
        if let Reply::Refused { message } = host.handle(load.clone()) {
            return Err(message);
        }
    }
    let local_value = shown(&host.handle(evaluate(formula)))?;
    let mut local_calls = Vec::new();
    let mut local_callbacks = Vec::new();
    for _ in 0..rounds {
        let t = Instant::now();
        shown(&host.handle(evaluate(formula)))?;
        local_calls.push(t.elapsed());
        let t = Instant::now();
        host.handle(Request::Recent);
        local_callbacks.push(t.elapsed());
    }
    drop(host);
    drop(set);

    // The add-in process.
    let mut dialogs = NoDialogs;
    let mut supervisor = Supervisor::new();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let started = Instant::now();
    let (incarnation, wire) = supervisor.launch(&exe, &["--sheet".into(), "Sheet1".into()])?;
    let mut remote = Remote::connect(incarnation, wire, Duration::from_secs(60), &mut || {
        pump_pending();
        true
    })?;
    let remote_startup = started.elapsed();
    let limit = Duration::from_secs(120);
    let mut pump = || {
        pump_pending();
        true
    };
    if let Some(load) = &load {
        if let Reply::Refused { message } = remote.call(load, &mut dialogs, limit, &mut pump) {
            return Err(message);
        }
    }
    let remote_value = shown(&remote.call(&evaluate(formula), &mut dialogs, limit, &mut pump))?;
    if remote_value != local_value {
        return Err(format!(
            "{formula}: in-process {local_value}, add-in process {remote_value}"
        ));
    }
    let mut remote_calls = Vec::new();
    let mut remote_callbacks = Vec::new();
    for _ in 0..rounds {
        let t = Instant::now();
        shown(&remote.call(&evaluate(formula), &mut dialogs, limit, &mut pump))?;
        remote_calls.push(t.elapsed());
        let t = Instant::now();
        match remote.call(&Request::Recent, &mut dialogs, limit, &mut pump) {
            Reply::Recent(list) if list.len() == 1 => {}
            other => return Err(format!("Recent: {other:?}")),
        }
        remote_callbacks.push(t.elapsed());
    }
    drop(remote);
    let finished = supervisor.finish(Duration::from_secs(20));
    let report = serde_json::json!({
        "formula": formula,
        "value": local_value,
        "xll": xll.map(|p| p.display().to_string()),
        "in_process": {
            "startup_ms": local_startup.as_secs_f64() * 1e3,
            "call_us": spread(local_calls),
            "callback_us": spread(local_callbacks),
        },
        "surrogate": {
            "startup_ms": remote_startup.as_secs_f64() * 1e3,
            "call_us": spread(remote_calls),
            "callback_us": spread(remote_callbacks),
            "ended": format!("{finished:?}"),
        },
    });
    println!("{report}");
    drop(shell);
    Ok(())
}
