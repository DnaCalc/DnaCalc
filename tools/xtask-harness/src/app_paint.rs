//! Excel pixels are a separate observation from calculation verdicts. Reuse the app server,
//! WebDriver session, upload route and committed RenderPlan; never synthesize painted values.
use super::*;

const HELP: &str = "cargo xtask app paint-compare calc --out DIR [--bundle DIR] [--workbook FILE] [--reference DIR] [--zooms 75,100,150] [--normal-font FAMILY,POINTS] [--locale TAG] [--browser chromium|firefox] [--decimal-separator TEXT] [--thousands-separator TEXT] [--sheet-height PX] [--excel EXE] [--desktop EXE]";

fn python() -> Command {
    Command::new(std::env::var_os("DNACALC_PAINT_PYTHON").unwrap_or_else(|| "python".into()))
}

/// chromedriver reads the product string of `/json/version` and refuses WebView2's `Edg/156...`.
/// A pass-through TCP proxy to the app's DevTools port rewrites that one field to the driver's own
/// version and relays every other byte (HTTP and WebSocket) untouched. The page, its protocol and
/// the pixels are the real WebView2's; the capture record names the substitution.
pub(super) fn devtools_proxy(upstream: u16, product: String) -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    thread::spawn(move || {
        for client in listener.incoming().flatten() {
            let product = product.clone();
            thread::spawn(move || {
                let Ok(mut up) = TcpStream::connect(("127.0.0.1", upstream)) else { return };
                let mut head = [0u8; 4096];
                let Ok(n) = client.peek(&mut head) else { return };
                let version = head[..n].starts_with(b"GET /json/version");
                if version {
                    // One plain request; answer it by hand with the product string replaced.
                    let mut client = client;
                    let mut request = vec![0u8; n];
                    let _ = client.read_exact(&mut request);
                    let _ = up.write_all(&request);
                    // Read the one response by its Content-Length: the server keeps the connection open.
                    let mut reply = Vec::new();
                    let mut chunk = [0u8; 4096];
                    loop {
                        let Ok(read) = up.read(&mut chunk) else { break };
                        if read == 0 { break }
                        reply.extend_from_slice(&chunk[..read]);
                        let text = String::from_utf8_lossy(&reply);
                        if let Some((head, body)) = text.split_once("\r\n\r\n") {
                            let wanted = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok())).unwrap_or(0);
                            if body.len() >= wanted { break }
                        }
                    }
                    let text = String::from_utf8_lossy(&reply).into_owned();
                    let text = rewrite_product(&text, &product);
                    let _ = client.write_all(text.as_bytes());
                    return;
                }
                let (mut a, mut b) = (client.try_clone().unwrap(), up.try_clone().unwrap());
                let forward = thread::spawn(move || { let _ = std::io::copy(&mut a, &mut b); let _ = b.shutdown(std::net::Shutdown::Write); });
                let (mut up, mut client) = (up, client);
                let _ = std::io::copy(&mut up, &mut client);
                let _ = forward.join();
            });
        }
    });
    Ok(port)
}

/// `"Browser": "Edg/156..."` becomes the given product, with Content-Length corrected.
fn rewrite_product(response: &str, product: &str) -> String {
    const BLANK: &str = "\r\n\r\n";
    let Some((head, body)) = response.split_once(BLANK) else { return response.to_owned() };
    let mut value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if value.get("Browser").is_none() {
        return response.to_owned();
    }
    value["Browser"] = json!(product);
    let body = value.to_string();
    let status = head.lines().next().unwrap_or("HTTP/1.1 200 OK");
    let headers = ["Content-Type: application/json; charset=UTF-8".to_owned(), format!("Content-Length: {}", body.len()), "Connection: close".to_owned()];
    format!("{status}{CRLF}{}{BLANK}{body}", headers.join(CRLF))
}

const CRLF: &str = "\r\n";

pub(super) fn run(ws: &Workspace, app: &App, raw: &[String]) -> ExitCode {
    match execute(ws, app, raw) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("PAINT failed: {error}\n{HELP}");
            ExitCode::FAILURE
        }
    }
}

fn execute(ws: &Workspace, app: &App, raw: &[String]) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("Excel paint capture requires Windows".into());
    }
    let mut out = None;
    let mut bundle_dir = None;
    let mut workbook = None;
    let mut reference = None;
    let mut excel_path = None;
    let mut normal_font = None;
    let mut locale=None;
    let mut decimal_separator=None;
    let mut thousands_separator=None;
    let mut browser=&BROWSERS[0];
    let mut sheet_height=None;
    let mut desktop_exe=None;
    let mut zooms = vec![100, 75, 150];
    let mut args = raw.iter();
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--out" => out = Some(PathBuf::from(value)),
            "--bundle" => bundle_dir = Some(PathBuf::from(value)),
            "--workbook" => workbook = Some(PathBuf::from(value)),
            "--reference" => reference = Some(PathBuf::from(value)),
            "--excel" => excel_path = Some(PathBuf::from(value)),
            "--desktop" => desktop_exe = Some(PathBuf::from(value)),
            "--locale" => {
                if value.is_empty() || !value.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-') {return Err("Locale needs a BCP 47 language tag".into());}
                locale=Some(value.clone());
            },
            "--decimal-separator"=>decimal_separator=Some(value.clone()),
            "--thousands-separator"=>thousands_separator=Some(value.clone()),
            "--browser"=>browser=BROWSERS.iter().find(|b|b.name==value.as_str()).ok_or_else(||format!("Unknown browser {value}; choose chromium or firefox"))?,
            "--sheet-height" => {
                let height=value.parse::<u32>().map_err(|e|e.to_string())?;
                if !(256..=4096).contains(&height) {return Err("Sheet height must be 256..4096 CSS pixels".into());}
                sheet_height=Some(height);
            }
            "--normal-font" => {
                let (family,points)=value.rsplit_once(',').ok_or("--normal-font needs FAMILY,POINTS")?;
                let points=points.parse::<f64>().map_err(|e|e.to_string())?;
                if family.is_empty() || !points.is_finite() || !(1. ..=409.).contains(&points) { return Err("Normal font needs a family and 1 to 409 points".into()); }
                normal_font=Some((family.to_owned(),points));
            }
            "--zooms" => {
                zooms = value
                    .split(',')
                    .map(|s| s.parse::<u32>().map_err(|e| e.to_string()))
                    .collect::<Result<_, _>>()?;
                if zooms.is_empty() || zooms.iter().any(|z| !(25..=200).contains(z)) {
                    return Err("zooms must be 25..200".into());
                }
            }
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    if decimal_separator.is_some()||thousands_separator.is_some() {
        let profile=calc_func::locale::FormatProfile::for_culture(locale.as_deref().unwrap_or("en-US")).ok_or("Unsupported regional locale")?;
        if profile.with_separators(decimal_separator.as_deref().unwrap_or(profile.decimal_separator),thousands_separator.as_deref().unwrap_or(profile.thousands_separator)).is_none() {
            return Err("Invalid regional separators".into());
        }
    }
    let out = out.ok_or("--out is required")?;
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let out = out.canonicalize().map_err(|e| e.to_string())?;
    let mut built_source = json!({"status":"unrecorded","reason":"prebuilt bundle; exact served asset hashes retained"});
    let bundle_dir = if let Some(dir) = bundle_dir {
        dir
    } else {
        let dir = ws.root.join(app.default_out);
        let before = runtime_source(ws)?;
        bundle(ws, app, &dir)?;
        if runtime_source(ws)? != before {
            return Err("runtime source changed while bundling; rebuild before capturing".into());
        }
        built_source = before;
        dir
    };
    let books = if let Some(path) = workbook {
        if normal_font.is_some() {return Err("--normal-font applies to generated inputs; an existing workbook keeps its own Normal font".into());}
        vec![path.canonicalize().map_err(|e| e.to_string())?]
    } else {
        let mut books = Vec::new();
        for kind in ["geometry", "text", "borders", "panes", "everyday"] {
            let path = out.join(format!("paint-{kind}.xlsx"));
            let bytes=if let Some((family,points))=&normal_font {calc_cases::fixtures::paint_with_normal(kind,family,*points)?}else{calc_cases::fixtures::paint(kind)?};
            fs::write(&path,bytes).map_err(|e| e.to_string())?;
            books.push(path);
        }
        books
    };
    let inputs: Vec<_> = books.iter().map(|p| Ok(json!({"path":p,"sha256":calc_verdict::sha256_hex(&fs::read(p).map_err(|e|e.to_string())?)}))).collect::<Result<_,String>>()?;
    let bootstrap = out.join("paint-bootstrap.xlsx");
    let bootstrap_bytes = calc_doc::blank_package();
    fs::write(&bootstrap, &bootstrap_bytes).map_err(|e| e.to_string())?;
    let request = json!({"books":inputs,"zooms":zooms,"out":out,"rows":18,"columns":10,"sheet_height":sheet_height,"bootstrap":{"path":bootstrap,"sha256":calc_verdict::sha256_hex(&bootstrap_bytes)}});
    fs::write(out.join("request.json"), request.to_string()).map_err(|e| e.to_string())?;
    let source = ws.root.join("tools/xtask-harness/src/paint");
    let reference_dir = reference
        .as_ref()
        .unwrap_or(&out)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if reference.is_some() && reference_dir == out {
        return Err(
            "--reference must differ from --out so the earlier browser captures remain intact"
                .into(),
        );
    }
    if reference.is_none() {
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-File"])
            .arg(source.join("excel.ps1"))
            .arg("-Request")
            .arg(out.join("request.json"));
        if let Some(path) = &excel_path {
            command.arg("-ExcelPath").arg(path);
        }
        let status = command
            .status()
            .map_err(|e| format!("Excel capture worker: {e}"))?;
        if !status.success() {
            return Err(format!("Excel capture worker exited {status}"));
        }
    }
    let refs: Value = serde_json::from_slice(
        &fs::read(reference_dir.join("excel.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let refs = refs.as_array().ok_or("Excel metadata is not an array")?;
    for (i, input) in inputs.iter().enumerate() {
        for zoom in &zooms {
            if !refs
                .iter()
                .any(|r| r["sha256"] == input["sha256"] && r["zoom"] == *zoom)
            {
                return Err(format!(
                    "no reference with the exact input hash and zoom for book {i}, {zoom}%"
                ));
            }
        }
    }
    let port = serve(bundle_dir.clone(), None)?;
    let mut url = format!("http://127.0.0.1:{port}/index.html?");
    let regional=[("locale",locale.as_ref()),("decimal-separator",decimal_separator.as_ref()),("thousands-separator",thousands_separator.as_ref())];
    for (key,value) in regional {if let Some(value)=value {
        let encoded=value.bytes().map(|b|format!("%{b:02X}")).collect::<String>();
        url.push_str(&format!("{key}={encoded}&"));
    }}
    let mut captures = Vec::new();
    // The same two painters, then again inside the WebView2 of `dna-calc-desktop --app` (the native
    // worker behind the page), attached to over its DevTools port. Only `--desktop EXE` runs it.
    let mut passes: Vec<(String, &str, Option<String>)> = ["dom", "canvas"].iter().map(|m| (m.to_string(), *m, None)).collect();
    let mut desktop_app = None;
    let mut desktop = json!({"status":"not_run","reason":"--desktop was not given"});
    if let Some(exe) = &desktop_exe {
        let port = free_port()?;
        let child = Command::new(exe)
            .args(["--app", "--url", "about:blank", "--debug-port", &port.to_string(), "--size", "1760x1100"])
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
        desktop_app = Some(DriverProcess { child, port });
        let driver_version = {
            let driver = variable_file(browser.variable).or_else(|| find_on_path(browser.program)).ok_or("no chromedriver")?;
            let text = cargo::text(&cargo::output(Command::new(driver).arg("--version"))?.stdout);
            text.split_whitespace().nth(1).ok_or("no chromedriver version")?.to_owned()
        };
        let upstream = port;
        let port = devtools_proxy(upstream, format!("Chrome/{driver_version}"))?;
        let started = Instant::now();
        while TcpStream::connect(("127.0.0.1", upstream)).is_err() {
            if started.elapsed() > Duration::from_secs(30) {
                return Err("the desktop app did not open its DevTools port".into());
            }
            thread::sleep(Duration::from_millis(200));
        }
        for mode in ["dom", "canvas"] {
            passes.push((format!("desktop-{mode}"), mode, Some(format!("127.0.0.1:{port}"))));
        }
        desktop = json!({"status":"captured","host":"dna-calc-desktop --app (WebView2, native workbook worker)","executable":exe,
            "executable_sha256":calc_verdict::sha256_hex(&fs::read(exe).map_err(|e|e.to_string())?),
            "driver":"chromedriver attached to the app's DevTools port through a proxy that only rewrites the product string of /json/version",
            "driver_version":driver_version,"webview2_product":"read from /json/version of the running runtime"});
    }
    for (label, mode, attach) in passes {
        let (_driver, session) = open_attached(browser, &out.join("downloads"), locale.as_deref(), attach.as_deref(), None)?;
        let result = (|| {
            for (book, input) in books.iter().zip(&inputs) {
                start_group(app, &session, &renderer::page(&url, Some(mode)))?;
                let browser_locale=session.script("return navigator.language;")?;
                if locale.as_deref().is_some_and(|expected|browser_locale.as_str().is_none_or(|actual|!actual.eq_ignore_ascii_case(expected))) {return Err(format!("Browser locale differs from requested locale: {browser_locale}"));}
                let reference = refs
                    .iter()
                    .find(|r| r["sha256"] == input["sha256"])
                    .ok_or("missing reference")?;
                let dpr = reference["dpi"].as_f64().ok_or("missing DPI")? / 96.;
                if browser.name=="firefox" {
                    let actual=session.script("return devicePixelRatio;")?.as_f64().ok_or("Missing browser display scale")?;
                    if (actual-dpr).abs()>1e-6 {return Err("Firefox and reference display scales differ; refusing resampling".into());}
                }
                capture_viewport(&session,browser,1200,dpr)?;
                upload(app, &session, "open-file", book)?;
                settled(
                    &session,
                    "return document.querySelector('[data-dnacalc=status]').textContent.startsWith('Opened ')",
                )?;
                if let Some(target)=sheet_height {
                    for _ in 0..3 {
                        let size=session.script("return [innerHeight,document.querySelector('[data-dnacalc=grid][data-grid-active=true]').clientHeight];")?;
                        let actual=size[1].as_i64().ok_or("Missing sheet height")?;
                        if actual==i64::from(target) {break;}
                        let height=size[0].as_i64().ok_or("Missing viewport height")?+i64::from(target)-actual;
                        if !(480..=8192).contains(&height) {return Err(format!("Requested sheet area needs an unsupported viewport height: {height}"));}
                        capture_viewport(&session,browser,height as u32,dpr)?;
                        paint_frames(&session)?;
                    }
                    settled(&session,&format!("const g=document.querySelector('[data-dnacalc=grid][data-grid-active=true]'),p=Object.values(window.__dnacalcRenderPlans||{{}}).map(f=>JSON.parse(f())).find(p=>p.active);return g&&g.clientHeight==={target}&&p&&p.pending===0&&p.plan.viewport.height==={target};"))?;
                }
                let visible = session.script("return [...document.querySelectorAll('[data-dnacalc=sheet-tab]')].map(t=>t.dataset.sheet);")?;
                let sheets = visible
                    .as_array()
                    .ok_or("visible sheet tabs were not returned")?;
                if sheets.is_empty() {
                    return Err("workbook has no visible sheet tabs".into());
                }
                for sheet in sheets {
                    for zoom in &zooms {
                        if !refs.iter().any(|r| {
                            r["sha256"] == input["sha256"]
                                && r["sheet"] == *sheet
                                && r["zoom"] == *zoom
                        }) {
                            return Err(format!(
                                "reference is incomplete: sheet {sheet}, zoom {zoom}%"
                            ));
                        }
                    }
                }
                for sheet in sheets {
                    session.script(&format!("const tabs=[...document.querySelectorAll('[data-dnacalc=sheet-tab]')].filter(t=>t.dataset.sheet==={sheet});if(tabs.length!==1)throw Error('worksheet tab is absent or ambiguous');tabs[0].click();return true;"))?;
                    settled(
                        &session,
                        &format!(
                            "return document.querySelector('[data-dnacalc=sheet-tab][aria-selected=true]')?.dataset.sheet==={sheet};"
                        ),
                    )?;
                    for zoom in &zooms {
                        session.script(&format!("const e=document.querySelector('[data-dnacalc=zoom-percent]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'{zoom}');e.dispatchEvent(new Event('input',{{bubbles:true}}));e.dispatchEvent(new Event('change',{{bubbles:true}}));return true;"))?;
                        settled(
                            &session,
                            &format!(
                                "const p=Object.values(window.__dnacalcRenderPlans||{{}}).map(f=>JSON.parse(f())).find(p=>p.active);return p&&p.pending===0&&!p.plan.interrupted&&Math.abs(p.plan.scale-{zoom}/100)<1e-9;"
                            ),
                        )?;
                        session.script("const g=document.querySelector('[data-dnacalc=grid][data-grid-active=true]');g.scrollTop=0;g.scrollLeft=0;g.dispatchEvent(new Event('scroll'));return true;")?;
                        settled(
                            &session,
                            "const p=Object.values(window.__dnacalcRenderPlans).map(f=>JSON.parse(f())).find(p=>p.active);return p&&p.plan.viewport.scroll_x===0&&p.plan.viewport.scroll_y===0;",
                        )?;
                        // Two animation frames after the committed plan; read-only inspection.
                        paint_frames(&session)?;
                        let mut plan=session.script("const p=Object.values(window.__dnacalcRenderPlans).map(f=>JSON.parse(f())).find(p=>p.active);p.layer_rect=document.querySelector('[data-dnacalc=grid][data-grid-active=true] .dc-layer').getBoundingClientRect().toJSON();p.dpr=devicePixelRatio;p.desktop_bridge=!!window.__dnacalcDesktop;p.fonts=[...new Set(p.plan.cells.map(c=>c.shown.style.family))].map(f=>({family:f,loaded:!f||document.fonts.check('11px '+JSON.stringify(f))}));return p;")?;
                        plan["browser"] = if browser.name=="firefox" {
                            session.script("return {userAgent:navigator.userAgent,platform:navigator.platform};")?
                        }else{session.post(
                            "/goog/cdp/execute",
                            json!({"cmd":"Browser.getVersion","params":{}}),
                        )?};
                        plan["locale"]=browser_locale.clone();
                        plan["regional_overrides"]=json!({"locale":locale,"decimal_separator":decimal_separator,"thousands_separator":thousands_separator});
                        let prefix = format!(
                            "{}-{}-{}-{}",
                            book.file_stem().unwrap().to_string_lossy(),
                            sheet.as_str().unwrap_or("sheet"),
                            label,
                            zoom
                        );
                        let screenshot = webdriver(
                            session.port,
                            "GET",
                            &format!("/session/{}/screenshot", session.id),
                            None,
                        )?;
                        fs::write(
                            out.join(format!("{prefix}.png.base64")),
                            screenshot.as_str().ok_or("no screenshot")?,
                        )
                        .map_err(|e| e.to_string())?;
                        plan["sha256"] = input["sha256"].clone();
                        plan["sheet"] = sheet.clone();
                        plan["zoom"] = json!(zoom);
                        plan["painter"] = json!(label);
                        plan["prefix"] = json!(prefix);
                        plan["png_base64_sha256"] = json!(calc_verdict::sha256_hex(
                            screenshot.as_str().ok_or("no screenshot")?.as_bytes()
                        ));
                        fs::write(out.join(format!("{prefix}.json")), plan.to_string())
                            .map_err(|e| e.to_string())?;
                        captures.push(plan);
                        println!("PAINT captured {prefix}");
                    }
                }
            }
            Ok::<_, String>(())
        })();
        session.end();
        result?;
    }
    let bundle_hashes: Vec<_>=fs::read_dir(&bundle_dir).map_err(|e|e.to_string())?.filter_map(Result::ok).filter(|e|e.path().is_file()).map(|e|Ok(json!({"name":e.file_name().to_string_lossy(),"sha256":calc_verdict::sha256_hex(&fs::read(e.path()).map_err(|e|e.to_string())?)}))).collect::<Result<_,String>>()?;
    let record = json!({"schema":"dnacalc-paint/1","captures":captures,"reference":reference_dir,"bundle_source":built_source,"workspace_head_at_capture":cargo::git(&ws.root,&["rev-parse","HEAD"])? .trim(),"workspace_diff_sha256":calc_verdict::sha256_hex(cargo::git(&ws.root,&["diff","HEAD"] )?.as_bytes()),"bundle":bundle_hashes,"desktop":desktop});
    drop(desktop_app);
    fs::write(out.join("browser.json"), record.to_string()).map_err(|e| e.to_string())?;
    let status = python()
        .arg(source.join("compare.py"))
        .arg(&out)
        .status()
        .map_err(|e| format!("cell parity reporting needs Python with Pillow and NumPy (DNACALC_PAINT_PYTHON selects the interpreter): {e}"))?;
    if !status.success() {
        return Err(format!("cell parity reporting exited {status}"));
    }
    Ok(())
}

fn capture_viewport(session:&Session,browser:&Browser,height:u32,dpr:f64)->Result<(),String> {
    if browser.name=="firefox" {
        session.viewport(1760,height)?;
    } else {
        session.post("/goog/cdp/execute",json!({"cmd":"Emulation.setDeviceMetricsOverride","params":{"width":1760,"height":height,"deviceScaleFactor":dpr,"mobile":false}}))?;
    }
    let actual=session.script("return [innerWidth,innerHeight];")?;
    if actual!=json!([1760,height]) {return Err(format!("Capture viewport differs from the requested size: {actual}"));}
    Ok(())
}

fn paint_frames(session:&Session)->Result<(),String> {
    session.post("/execute/async",json!({"script":"const done=arguments[arguments.length-1];document.fonts.ready.then(()=>requestAnimationFrame(()=>requestAnimationFrame(()=>done(true))));","args":[]}))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Native-only: runs Python against synthetic raster controls without starting Excel or a browser.
    #[test]
    fn cell_measurements_reject_known_paint_damage() {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/paint/compare.py");
        let output = python()
            .arg(script)
            .arg("--self-test")
            .output()
            .expect("Python with Pillow and NumPy is required by paint-compare");
        assert!(output.status.success(), "{}{}", cargo::text(&output.stdout), cargo::text(&output.stderr));
    }
}

fn runtime_source(ws: &Workspace) -> Result<Value, String> {
    let diff = cargo::git(
        &ws.root,
        &[
            "diff",
            "HEAD",
            "--",
            "apps",
            "crates",
            "Cargo.toml",
            "Cargo.lock",
        ],
    )?;
    Ok(
        json!({"status":"built_by_this_command","head":cargo::git(&ws.root,&["rev-parse","HEAD"] )?.trim(),"diff_sha256":calc_verdict::sha256_hex(diff.as_bytes()),
            "profile":{"name":"web","opt_level_override":std::env::var("CARGO_PROFILE_WEB_OPT_LEVEL").ok(),"lto_override":std::env::var("CARGO_PROFILE_WEB_LTO").ok(),"codegen_units_override":std::env::var("CARGO_PROFILE_WEB_CODEGEN_UNITS").ok()}}),
    )
}

fn settled(session: &Session, script: &str) -> Result<(), String> {
    let start = Instant::now();
    loop {
        if session.script(script)?.as_bool() == Some(true) {
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(30) {
            return Err(format!("paint did not settle: {script}"));
        }
        thread::sleep(Duration::from_millis(100));
    }
}
