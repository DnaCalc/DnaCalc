//! Measurements extend the existing workbook smoke, using its browser, upload and input route.
use super::*;

pub(super) fn page(page: &str, mode: Option<&str>) -> String {
    let Some(mode) = mode else { return page.into() };
    let (base, query) = page.split_once('?').unwrap_or((page, ""));
    let mut parts: Vec<_> = query
        .split('&')
        .filter(|p| !p.is_empty() && !p.starts_with("renderer="))
        .collect();
    let painter = format!("renderer={mode}");
    parts.push(&painter);
    format!("{base}?{}", parts.join("&"))
}
pub(super) fn prepare(session: &Session, browser: &str) -> Result<(), String> {
    if browser == "firefox" {
        session.viewport(1440, 1000)?;
    } else {
        session.post("/goog/cdp/execute",json!({"cmd":"Emulation.setDeviceMetricsOverride","params":{"width":1440,"height":1000,"deviceScaleFactor":1,"mobile":false}}))?;
    }
    session.post("/timeouts", json!({"script":120_000}))?;
    session.script(include_str!(
        "../../../apps/dna-calc/tests/smoke/renderer.js"
    ))?;
    Ok(())
}
pub(super) fn after_open(session: &Session) -> Result<(), String> {
    session.script("return window.__rendererMeasurement.afterOpen().then(()=>true);")?;
    Ok(())
}
pub(super) fn finish(
    session: &Session,
    browser: &str,
    workbook: &Path,
    dir: &Path,
) -> Result<(), String> {
    let mut value = session.script("return window.__rendererMeasurement.finish();")?;
    value["browser_rss_bytes"] = json!(browser_rss(session));
    if browser == "chromium" {
        for (key, cmd) in [
            ("cdp_heap", "Runtime.getHeapUsage"),
            ("cdp_dom", "Memory.getDOMCounters"),
            ("cdp_accessibility", "Accessibility.getFullAXTree"),
        ] {
            value[key] = session
                .post("/goog/cdp/execute", json!({"cmd":cmd,"params":{}}))
                .unwrap_or_else(|e| json!({"error":e}));
        }
    }
    value["workbook"] = json!(workbook.display().to_string());
    value["host"] = json!(std::env::consts::OS);
    value["workbook_sha256"] = json!(calc_verdict::sha256_hex(
        &fs::read(workbook).map_err(|e| e.to_string())?
    ));
    value["source_head"] = json!(
        cargo::git(
            &std::env::current_dir().map_err(|e| e.to_string())?,
            &["rev-parse", "HEAD"]
        )?
        .trim()
    );
    let diff = cargo::git(
        &std::env::current_dir().map_err(|e| e.to_string())?,
        &[
            "diff",
            "HEAD",
            "--",
            "apps/dna-calc",
            "apps/dna-calc-model",
            "apps/dna-calc-protocol",
            "apps/dna-calc-worker",
            "apps/dna-formula-ui",
            "crates/calc-host",
            "tools/xtask-harness",
        ],
    )?;
    value["source_dirty"] = json!(!diff.is_empty());
    value["source_diff_sha256"] = json!(calc_verdict::sha256_hex(diff.as_bytes()));
    let painter = value
        .pointer("/after_scroll/painter")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let trial = value["trial"].as_u64().unwrap_or(0);
    let stem = workbook.file_stem().unwrap_or_default().to_string_lossy();
    let prefix = format!("{stem}-{browser}-{painter}-{trial}");
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    fs::write(
        dir.join(format!("{prefix}.json")),
        serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // WebDriver screenshots retain pixels, including text at each zoom. Base64 is kept in
    // an explicit artifact rather than adding a second image codec to the harness.
    for zoom in [50, 100, 125, 200, 400] {
        let input = select(&APPS[1], "zoom-percent");
        act_click(&APPS[1], session, "zoom-percent")?;
        session.keys(&input, "\u{E009}a\u{E000}")?;
        session.keys(&input, &format!("\u{E003}{zoom}{ENTER}"))?;
        session.script(&format!("return window.__rendererMeasurement.wait(()=>{{var p=Object.values(window.__dnacalcRenderPlans).map(f=>JSON.parse(f())).find(p=>p.active);return p&&p.pending===0&&!p.plan.interrupted&&Math.abs(p.plan.scale-{zoom}/100)<1e-9;}}).then(()=>true);"))?;
        let screenshot = webdriver(
            session.port,
            "GET",
            &format!("/session/{}/screenshot", session.id),
            None,
        )?;
        fs::write(
            dir.join(format!("{prefix}-zoom{zoom}.png.base64")),
            screenshot.as_str().ok_or("no screenshot")?,
        )
        .map_err(|e| e.to_string())?;
        let plan=session.script("var p=Object.values(window.__dnacalcRenderPlans).map(f=>JSON.parse(f())).find(p=>p.active);p.layer_rect=document.querySelector('[data-dnacalc=grid][data-grid-active=true] .dc-layer').getBoundingClientRect().toJSON();return p;")?;
        fs::write(
            dir.join(format!("{prefix}-zoom{zoom}.plan.json")),
            serde_json::to_vec(&plan).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    println!(
        "RENDERER {browser} {prefix}: {}",
        dir.join(format!("{prefix}.json")).display()
    );
    Ok(())
}

pub(super) fn run_measurements(
    _ws: &Workspace,
    app: &App,
    browser: &Browser,
    page_url: &str,
    run: &WorkbookRun,
) -> Result<(usize, usize), String> {
    let failed = 0;
    for trial in 0..3 {
        for mode in if trial % 2 == 0 {
            ["dom", "canvas"]
        } else {
            ["canvas", "dom"]
        } {
            let (_driver, session) = open(browser)?;
            let result = (|| {
                start_group(app, &session, &page(page_url, Some(mode)))?;
                session.script(&format!("window.__rendererTrial={trial};return null;"))?;
                Ok::<_, String>(run_workbook(app, &session, browser.name, run))
            })();
            let _ = webdriver(
                session.port,
                "DELETE",
                &format!("/session/{}", session.id),
                None,
            );
            let failures = result?;
            if !failures.is_empty() {
                return Err(format!(
                    "RENDERER {} {mode} trial {trial}: {}",
                    browser.name,
                    failures.join("; ")
                ));
            }
        }
    }
    Ok((6 - failed, failed))
}
/// Linux sum of resident bytes for the browser processes started by this driver, including
/// worker/content processes. Shared pages are counted per process; this is RSS, not private RAM.
fn browser_rss(session: &Session) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let mut processes = Vec::new();
        for entry in fs::read_dir("/proc").ok()?.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            let Some((_, rest)) = stat.rsplit_once(')') else {
                continue;
            };
            let fields: Vec<_> = rest.split_whitespace().collect();
            if let (Some(ppid), Some(pages)) = (
                fields.get(1).and_then(|s| s.parse::<u32>().ok()),
                fields.get(21).and_then(|s| s.parse::<u64>().ok()),
            ) {
                processes.push((pid, ppid, pages));
            }
        }
        let mut owned = std::collections::BTreeSet::from([session.driver_pid]);
        loop {
            let count = owned.len();
            for (pid, ppid, _) in &processes {
                if owned.contains(ppid) {
                    owned.insert(*pid);
                }
            }
            if owned.len() == count {
                break;
            }
        }
        Some(
            processes
                .iter()
                .filter(|(pid, _, _)| *pid != session.driver_pid && owned.contains(pid))
                .map(|(_, _, pages)| pages * 4096)
                .sum(),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = session;
        None
    }
}

/// Core assertions use retained data and actual raster ink when cells have no DOM node.
/// Returning None preserves the existing DOM assertion route for every other surface.
pub(super) fn core_observation(
    app: &App,
    session: &Session,
    check: &Check,
) -> Result<Option<Value>, String> {
    let (name, mut spec) = match check {
        Check::HashMask(address) => ("", json!({"op":"hash","address":address})),
        Check::Text(n, _) | Check::TextContains(n, _) | Check::Starts(n, _) => {
            (*n, json!({"op":"text"}))
        }
        Check::FitsText(n) => (*n, json!({"op":"fits"})),
        Check::Absent(n) => (*n, json!({"op":"absent"})),
        Check::Count(n, _) => (*n, json!({"op":"count"})),
        Check::Attr(n, field, _) => (*n, json!({"op":"attribute","field":field})),
        Check::Spill(n, first, last) => (*n, json!({"op":"spill","first":first,"last":last})),
        Check::Aligned(n, h, v) => (*n, json!({"op":"aligned","horizontal":h,"vertical":v})),
        Check::Style(n, d) | Check::Computed(n, d) => (*n, json!({"op":"style","declarations":d})),
        _ => return Ok(None),
    };
    if !name.is_empty() {
        let selector = select(app, name);
        let paint = selector
            .chars()
            .filter(|c| *c != char::from(34) && *c != char::from(39))
            .collect::<String>();
        let cell = paint.contains("[data-dnacalc=cell]");
        let decoration = paint.contains("[data-dnacalc=cell-decoration]")
            || paint.contains("[data-dnacalc=cell-borders]");
        if !cell && !decoration {
            return Ok(None);
        }
        if !selector.contains("data-cell=") {
            if selector == select(app, "cell")
                && matches!(check, Check::Count(..) | Check::Absent(..))
            {
                spec["op"] = json!(if matches!(check, Check::Absent(..)) {
                    "empty"
                } else {
                    "count"
                });
            } else {
                return Ok(None);
            }
        }
        spec["selector"] = json!(selector);
    }
    let (_, js) = include_str!("../../../apps/dna-calc/tests/smoke/renderer.js")
        .split_once("// Core observations, independent of measurements.")
        .expect("core observation script");
    let value=session.script(&format!("if(!window.__rendererCoreCheck){{(function(){{{js}}})()}} return window.__rendererCoreCheck({spec});"))?;
    Ok(value["handled"]
        .as_bool()
        .unwrap_or(false)
        .then(|| value["seen"].clone()))
}
