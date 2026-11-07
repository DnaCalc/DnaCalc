//! App mode (`dna-onecalc-desktop --app`): the page the browsers run, hosted in WebView2, with the
//! session native behind the page.
//!
//! The shell is skin deep. The page is OneCalc's surface over a bridge link
//! ([`dna_onecalc::bridge`]); this module carries each request string the page posts to
//! [`Host::answer`] on the STA thread (the one that owns add-in calls) and posts the answer back.
//! The window chain, the WebView2 host and its string bridge are `calc-platform-desktop`'s, the
//! add-in set `calc-addin-host`'s, and the file dialogs `calc-platform-desktop`'s: nothing here is
//! OneCalc-only beyond wiring them to the host.
use crate::host::{Files, Host};
use calc_addin_host::{AddinHost, Document};
use calc_platform_desktop::dialogs::{self, FileFilter};
use calc_platform_desktop::{
    Shell, ThreadMessage, ThreadTimer, WebView, WebViewOptions, per_monitor_dpi_aware,
    run_message_loop,
};
use dna_onecalc::{AddinRef, Session};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};

/// What the command line asks of app mode.
struct Options {
    /// Navigate here (the harness's bundle server), instead of mapping `assets`.
    url: Option<String>,
    /// The built page (`cargo xtask app bundle onecalc`), mapped as `https://dnacalc.app/`.
    assets: Option<PathBuf>,
    xlls: Vec<PathBuf>,
    debug_port: Option<u16>,
    size: (i32, i32),
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        url: None,
        assets: None,
        xlls: Vec::new(),
        debug_port: None,
        size: (1280, 900),
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().cloned().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--app" => {}
            "--url" => o.url = Some(value()?),
            "--assets" => o.assets = Some(PathBuf::from(value()?)),
            "--xll" => o.xlls.push(PathBuf::from(value()?)),
            "--debug-port" => {
                o.debug_port = Some(value()?.parse().map_err(|_| "--debug-port needs a port")?)
            }
            "--size" => {
                let text = value()?;
                let (w, h) = text.split_once('x').ok_or("--size needs WIDTHxHEIGHT")?;
                o.size = (
                    w.parse().map_err(|_| "--size width")?,
                    h.parse().map_err(|_| "--size height")?,
                );
            }
            other => return Err(format!("unknown app argument: {other}")),
        }
    }
    if o.url.is_none() && o.assets.is_none() {
        return Err("app mode needs --assets DIR (the built page) or --url URL".into());
    }
    Ok(o)
}

/// The workbook name the add-ins see.
const WORKBOOK: &str = "Book1";

/// The timer that follows the window's size (milliseconds).
const TICK_MS: u32 = 50;
const TICK_ID: usize = 0x4f4e43;

/// The native file dialogs, owned by the window.
struct Dialogs {
    owner: *mut std::ffi::c_void,
}

const SESSION_FILES: [FileFilter<'static>; 2] = [
    FileFilter {
        name: "OneCalc sessions (*.xlsx)",
        patterns: "*.xlsx",
    },
    FileFilter {
        name: "All files",
        patterns: "*.*",
    },
];

impl Files for Dialogs {
    fn choose_open(&mut self) -> Result<Option<(String, Vec<u8>)>, String> {
        let Some(path) = dialogs::open_file(self.owner.cast(), "Open a session", &SESSION_FILES)?
        else {
            return Ok(None);
        };
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Some((name, bytes)))
    }

    fn choose_save(&mut self, _suggested: &str, bytes: &[u8]) -> Result<Option<String>, String> {
        let Some(path) =
            dialogs::save_file(self.owner.cast(), "Save the session", &SESSION_FILES, "xlsx")?
        else {
            return Ok(None);
        };
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Some(path.display().to_string()))
    }
}

struct App {
    host: RefCell<Host<Dialogs>>,
    web: WebView,
    frame: *mut std::ffi::c_void,
    bounds: Cell<(i32, i32)>,
}

impl App {
    /// One request from the page: answered with the same id.
    fn request(&self, message: &str) {
        let answer = self.host.borrow_mut().answer(message);
        match answer {
            Some(text) => {
                if let Err(why) = self.web.post(&text, None) {
                    eprintln!("dna-onecalc-desktop: {why}");
                }
            }
            None => eprintln!("dna-onecalc-desktop: a message that is no call was ignored"),
        }
    }

    /// The window's size, once per tick.
    fn tick(&self) {
        if let Some((width, height)) = calc_platform_desktop::client_size(self.frame.cast()) {
            if (width, height) != self.bounds.get() && width > 0 && height > 0 {
                self.bounds.set((width, height));
                self.web.set_bounds(width, height);
            }
        }
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let o = options(args)?;
    per_monitor_dpi_aware();
    let shell = Shell::new("DNA OneCalc")?;
    let frame = shell.workbook_hwnd();
    shell.show(o.size.0, o.size.1);
    let user_data = std::env::var_os("DNACALC_WEBVIEW2_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("dnacalc-onecalc-webview2"));
    let mut browser_args = String::new();
    if let Some(port) = o.debug_port {
        browser_args.push_str(&format!(
            " --remote-debugging-port={port} --remote-allow-origins=*"
        ));
    }
    let web = WebView::create(
        frame,
        &WebViewOptions {
            user_data: &user_data,
            browser_args,
            size: o.size,
        },
    )?;

    let session = Session::new("Sheet1").map_err(|e| format!("{e:?}"))?;
    let (mut addins, mut loaded) = (None, Vec::new());
    let mut host = Host::new(session, Dialogs { owner: frame.cast() });
    if !o.xlls.is_empty() {
        let sheets: Vec<(calc_value::SheetId, String)> = host
            .session()
            .sheets()
            .map(|(name, id)| (id, name.to_owned()))
            .collect();
        let sheet_refs: Vec<(calc_value::SheetId, &str)> =
            sheets.iter().map(|(id, name)| (*id, name.as_str())).collect();
        let mut set = AddinHost::new(
            &shell,
            &Document {
                workbook: WORKBOOK,
                active_sheet: Some("Sheet1"),
                sheets: &sheet_refs,
            },
        )?;
        for xll in &o.xlls {
            set.load(xll)?;
            let bytes = std::fs::read(xll).map_err(|e| format!("{}: {e}", xll.display()))?;
            loaded.push(AddinRef {
                kind: "xll".to_owned(),
                path: xll.display().to_string(),
                hash: format!("sha256:{}", calc_verdict::sha256_hex(&bytes)),
            });
        }
        host.session_mut().set_user_functions(set.callables()?);
        addins = Some(set);
    }
    host.set_loaded(loaded);
    // Declared after the shell so that it drops first: the add-ins close while their windows exist.
    let app = Rc::new(App {
        host: RefCell::new(host),
        web,
        frame: frame.cast(),
        bounds: Cell::new(o.size),
    });
    {
        // The page's messages must not keep the app alive: a weak handle lets it drop at exit.
        let weak = Rc::downgrade(&app);
        app.web.on_message(move |message| {
            if let Some(app) = weak.upgrade() {
                app.request(&message);
            }
        })?;
    }
    let target = if let Some(url) = &o.url {
        url.clone()
    } else {
        let assets = o.assets.as_ref().ok_or("no assets")?;
        app.web.map_folder("dnacalc.app", assets)?;
        "https://dnacalc.app/index.html".to_owned()
    };
    app.web.navigate(&target)?;
    let timer = ThreadTimer::start(TICK_ID, TICK_MS);
    // The loop ends when the window is closed.
    run_message_loop(
        || calc_platform_desktop::client_size(frame).is_none(),
        |message| {
            if message == ThreadMessage::Timer(TICK_ID) {
                app.tick();
            }
        },
    );
    drop(timer);
    drop(app);
    drop(addins);
    drop(shell);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: argument parsing for app mode.
    #[test]
    fn app_options_need_a_page() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(options(&args(&["--app"])).is_err());
        let o = options(&args(&[
            "--app", "--assets", "dist", "--size", "800x600", "--xll", "a.xll",
        ]))
        .unwrap();
        assert_eq!((o.size, o.xlls.len()), ((800, 600), 1));
        assert!(options(&args(&["--app", "--url", "http://127.0.0.1:1/"])).is_ok());
        assert!(options(&args(&["--app", "--bogus"])).is_err());
    }
}
