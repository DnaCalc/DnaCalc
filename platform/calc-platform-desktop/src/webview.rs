//! The WebView2 host the desktop applications show their page in, and the string and bytes bridge
//! between that page and the native side.
//!
//! The host is skin deep. It creates the browser inside a window, loads a page (a mapped folder or
//! a URL), and carries text between the page's `chrome.webview.postMessage` and
//! [`WebView::on_message`] / [`WebView::post`]. A message may carry bytes after the
//! [`SEPARATOR`], base64 encoded: the page side is `apps/dna-calc/web/desktop-bridge.js`. What the
//! messages mean is the application's.
use crate::assets::{Assets, content_type, requested_name};
use std::result::Result;
use std::{path::Path, rc::Rc};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    Win32::Foundation::HWND as WinHwnd,
    core::{Interface, PCWSTR, PWSTR},
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The text of `bytes` in standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The value of each base64 digit, 255 for a byte that is none.
const DIGITS: [u8; 256] = {
    let mut table = [255u8; 256];
    let mut i = 0;
    while i < 64 {
        table[BASE64[i] as usize] = i as u8;
        i += 1;
    }
    table
};

/// The bytes of base64 `text`, or `None` when it holds a byte that is no digit.
pub fn unbase64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut bits, mut have) = (0u32, 0);
    for c in text.bytes().take_while(|&c| c != b'=') {
        let v = DIGITS[c as usize];
        if v == 255 {
            return None;
        }
        bits = (bits << 6) | u32::from(v);
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push((bits >> have) as u8);
            bits &= (1 << have) - 1;
        }
    }
    Some(out)
}

/// The separator between a message's text and its base64 bytes (see desktop-bridge.js).
pub const SEPARATOR: char = '\u{1}';

/// A message from the page split into its text and its bytes, when it carries any.
pub fn split_message(message: &str) -> (&str, Option<Vec<u8>>) {
    match message.split_once(SEPARATOR) {
        Some((text, encoded)) => (text, unbase64(encoded)),
        None => (message, None),
    }
}

/// How a [`WebView`] is made.
pub struct WebViewOptions<'a> {
    /// The browser's user data folder.
    pub user_data: &'a Path,
    /// Extra Chromium switches (for example a remote debugging port).
    pub browser_args: String,
    /// The first client size in pixels.
    pub size: (i32, i32),
}

/// A WebView2 browser inside a window.
pub struct WebView {
    _environment: ICoreWebView2Environment,
    controller: ICoreWebView2Controller,
    view: ICoreWebView2,
}

fn environment(user_data: &Path, arguments: String) -> Result<ICoreWebView2Environment, String> {
    let options = CoreWebView2EnvironmentOptions::default();
    unsafe { options.set_additional_browser_arguments(arguments) };
    let options: ICoreWebView2EnvironmentOptions = options.into();
    let (tx, rx) = std::sync::mpsc::channel();
    let folder = CoTaskMemPWSTR::from(user_data.to_string_lossy().as_ref());
    CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                *folder.as_ref().as_pcwstr(),
                &options,
                &handler,
            )
            .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |code, environment| {
            code?;
            let _ = tx.send(environment);
            Ok(())
        }),
    )
    .map_err(|e| format!("WebView2 environment: {e:?}"))?;
    rx.recv()
        .ok()
        .flatten()
        .ok_or_else(|| "WebView2 gave no environment".to_owned())
}

fn controller(
    environment: &ICoreWebView2Environment,
    parent: WinHwnd,
) -> Result<ICoreWebView2Controller, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let environment = environment.clone();
    CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            environment
                .CreateCoreWebView2Controller(parent, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |code, controller| {
            code?;
            let _ = tx.send(controller);
            Ok(())
        }),
    )
    .map_err(|e| format!("WebView2 controller: {e:?}"))?;
    rx.recv()
        .ok()
        .flatten()
        .ok_or_else(|| "WebView2 gave no controller".to_owned())
}

impl WebView {
    /// Creates the browser in `parent`, sized and visible. The calling thread must have entered
    /// a single-threaded apartment ([`crate::Shell::new`] does).
    pub fn create(
        parent: windows_sys::Win32::Foundation::HWND,
        options: &WebViewOptions<'_>,
    ) -> Result<Self, String> {
        let environment = environment(options.user_data, options.browser_args.trim().to_owned())?;
        let controller = controller(&environment, WinHwnd(parent.cast()))?;
        let view = unsafe { controller.CoreWebView2() }.map_err(|e| e.to_string())?;
        let this = Self {
            _environment: environment,
            controller,
            view,
        };
        this.set_bounds(options.size.0, options.size.1);
        let _ = unsafe { this.controller.SetIsVisible(true) };
        Ok(this)
    }

    /// Makes the browser fill `width` by `height` client pixels.
    pub fn set_bounds(&self, width: i32, height: i32) {
        let bounds = windows::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        let _ = unsafe { self.controller.SetBounds(bounds) };
    }

    /// Runs `source` in every document before its own scripts (the page side of the bridge).
    pub fn add_script(&self, source: &str) -> Result<(), String> {
        let view = self.view.clone();
        let source = source.to_owned();
        AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| unsafe {
                let js = CoTaskMemPWSTR::from(source.as_str());
                view.AddScriptToExecuteOnDocumentCreated(*js.as_ref().as_pcwstr(), &handler)
                    .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(|code, _| code),
        )
        .map_err(|e| format!("WebView2 script: {e:?}"))
    }

    /// Serves `folder` as `https://<host>/`, so a built page loads without a server.
    pub fn map_folder(&self, host: &str, folder: &Path) -> Result<(), String> {
        let view = self
            .view
            .cast::<ICoreWebView2_3>()
            .map_err(|e| e.to_string())?;
        let host = CoTaskMemPWSTR::from(host);
        let folder = CoTaskMemPWSTR::from(folder.to_string_lossy().as_ref());
        unsafe {
            view.SetVirtualHostNameToFolderMapping(
                *host.as_ref().as_pcwstr(),
                *folder.as_ref().as_pcwstr(),
                COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_ALLOW,
            )
        }
        .map_err(|e| e.to_string())
    }

    /// Serves `assets` as `https://<host>/`, answering the page's requests from memory: a page
    /// that is embedded in the executable loads with no folder and no server. A request for a
    /// file the block does not hold gets a 404.
    pub fn serve_assets(&self, host: &str, assets: Rc<Assets>) -> Result<(), String> {
        let filter = format!("https://{host}/*");
        let wide = CoTaskMemPWSTR::from(filter.as_str());
        unsafe {
            self.view.AddWebResourceRequestedFilter(
                *wide.as_ref().as_pcwstr(),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
            )
        }
        .map_err(|e| e.to_string())?;
        let environment = self._environment.clone();
        let prefix = format!("https://{host}");
        let mut token = 0i64;
        unsafe {
            self.view
                .add_WebResourceRequested(
                    &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else { return Ok(()) };
                        let request = args.Request()?;
                        let mut uri = PWSTR::null();
                        request.Uri(&mut uri)?;
                        let uri = take_pwstr(uri);
                        let found = uri
                            .strip_prefix(&prefix)
                            .and_then(requested_name)
                            .and_then(|name| assets.get(&name).map(|bytes| (name, bytes)));
                        let (status, reason, kind, bytes): (i32, &str, &str, &[u8]) = match &found {
                            Some((name, bytes)) => (200, "OK", content_type(name), bytes),
                            None => (404, "Not Found", "text/plain", b"not found"),
                        };
                        let stream = windows::Win32::UI::Shell::SHCreateMemStream(Some(bytes));
                        let headers = format!(
                            "Content-Type: {kind}
Cache-Control: no-store
"
                        );
                        let headers = CoTaskMemPWSTR::from(headers.as_str());
                        let reason = CoTaskMemPWSTR::from(reason);
                        let response = environment.CreateWebResourceResponse(
                            stream.as_ref(),
                            status,
                            *reason.as_ref().as_pcwstr(),
                            *headers.as_ref().as_pcwstr(),
                        )?;
                        args.SetResponse(&response)
                    })),
                    &mut token,
                )
                .map_err(|e| e.to_string())
        }
    }

    /// Calls `handler` with the page's title, each time it changes (`document.title`).
    pub fn on_title_changed(&self, handler: impl Fn(String) + 'static) -> Result<(), String> {
        let mut token = 0i64;
        unsafe {
            self.view
                .add_DocumentTitleChanged(
                    &DocumentTitleChangedEventHandler::create(Box::new(move |sender, _| {
                        if let Some(view) = sender {
                            let mut title = PWSTR::null();
                            if view.DocumentTitle(&mut title).is_ok() {
                                handler(take_pwstr(title));
                            }
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| e.to_string())
        }
    }

    /// Calls `handler` with each string the page posts (`chrome.webview.postMessage`).
    pub fn on_message(&self, handler: impl Fn(String) + 'static) -> Result<(), String> {
        let handler = Rc::new(handler);
        let mut token = 0i64;
        unsafe {
            self.view
                .add_WebMessageReceived(
                    &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                        if let Some(args) = args {
                            let mut message = PWSTR::null();
                            if args.TryGetWebMessageAsString(&mut message).is_ok() {
                                handler(take_pwstr(message));
                            }
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|e| e.to_string())
        }
    }

    /// Posts `text` to the page, followed by `bytes` after the [`SEPARATOR`] when there are any.
    pub fn post(&self, text: &str, bytes: Option<&[u8]>) -> Result<(), String> {
        let message = match bytes {
            None => text.to_owned(),
            Some(bytes) => format!("{text}{SEPARATOR}{}", base64(bytes)),
        };
        let wide = CoTaskMemPWSTR::from(message.as_str());
        unsafe { self.view.PostWebMessageAsString(*wide.as_ref().as_pcwstr()) }
            .map_err(|e| format!("post to page failed: {e}"))
    }

    /// Saves every download the page starts into `folder`, under the name the browser proposed,
    /// without asking: the save path of a run that has no person to choose a file (the smoke
    /// suite's `--save-dir`). A file of the same name is replaced.
    pub fn save_downloads_to(&self, folder: &Path) -> Result<(), String> {
        let view = self
            .view
            .cast::<ICoreWebView2_4>()
            .map_err(|e| e.to_string())?;
        // WebView2 wants a full backslash path, not a canonical verbatim one.
        let folder = folder.canonicalize().map_err(|e| e.to_string())?;
        let text = folder.to_string_lossy();
        let folder = std::path::PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text));
        let mut token = 0i64;
        unsafe {
            view.add_DownloadStarting(
                &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        let mut proposed = PWSTR::null();
                        if args.ResultFilePath(&mut proposed).is_ok() {
                            let proposed = take_pwstr(proposed);
                            let name = proposed_name(Path::new(&proposed));
                            if let Some(name) = name {
                                let target = folder.join(name);
                                let _ = std::fs::remove_file(&target);
                                let wide = CoTaskMemPWSTR::from(target.to_string_lossy().as_ref());
                                if let Err(why) = args.SetResultFilePath(*wide.as_ref().as_pcwstr())
                                {
                                    eprintln!(
                                        "dna-calc-desktop: cannot save to {}: {why}",
                                        target.display()
                                    );
                                }
                                let _ = args.SetHandled(true);
                            }
                        }
                    }
                    Ok(())
                })),
                &mut token,
            )
            .map_err(|e| e.to_string())
        }
    }

    /// Loads `url` (a web address or a mapped folder's).
    pub fn navigate(&self, url: &str) -> Result<(), String> {
        let wide = CoTaskMemPWSTR::from(url);
        unsafe { self.view.Navigate(*wide.as_ref().as_pcwstr()) }.map_err(|e| e.to_string())
    }
}

/// The version of the WebView2 Evergreen runtime installed on this machine, or `None` when there
/// is none (a runtime in the folder `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` names is the one asked
/// about).
pub fn runtime_version() -> Option<String> {
    let mut version = PWSTR::null();
    unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version) }.ok()?;
    let version = take_pwstr(version);
    (!version.is_empty()).then_some(version)
}

/// Where a person downloads the WebView2 Evergreen runtime.
pub const RUNTIME_DOWNLOAD: &str =
    "https://developer.microsoft.com/microsoft-edge/webview2/#download-section";

/// The name the page gave a download. WebView2 proposes a path that does not clash with a file in
/// the user's Downloads folder, so "Book1.xlsx" arrives as "Book1 (1).xlsx" when that exists: the
/// number is taken off again when the unnumbered name is a file beside it.
fn proposed_name(proposed: &Path) -> Option<std::ffi::OsString> {
    let name = proposed.file_name()?.to_string_lossy().into_owned();
    let (stem, extension) = match name.rfind('.') {
        Some(dot) => (&name[..dot], &name[dot..]),
        None => (name.as_str(), ""),
    };
    // Edge numbers a clash with a timestamp: "Book1 - 2026-11-09T011151.319.xlsx".
    if let Some(at) = stem.rfind(" - ") {
        if is_clash_stamp(&stem[at + 3..]) {
            return Some(format!("{}{extension}", &stem[..at]).into());
        }
    }
    if let Some(open) = stem.rfind(" (") {
        let number = &stem[open + 2..];
        if let Some(digits) = number.strip_suffix(')') {
            let plain = format!("{}{extension}", &stem[..open]);
            if !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
                && proposed.with_file_name(&plain).exists()
            {
                return Some(plain.into());
            }
        }
    }
    Some(name.into())
}

/// Whether `text` is `YYYY-MM-DDThhmmss.mmm`, the stamp Edge appends to a name already taken.
fn is_clash_stamp(text: &str) -> bool {
    let shape = b"dddd-dd-ddTdddddd.ddd";
    text.len() == shape.len()
        && text.bytes().zip(shape).all(|(b, &p)| {
            if p == b'd' {
                b.is_ascii_digit()
            } else {
                b == p
            }
        })
}

/// Makes the process render at each display's real scale. Call before the first window.
pub fn per_monitor_dpi_aware() {
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }
}

thread_local! {
    /// The ids Windows gave the calling thread's timers, with the ids their owners asked for: a
    /// timer with no window gets an id of the system's own choosing, whatever was asked.
    static TIMERS: std::cell::RefCell<Vec<(usize, usize)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A repeating timer on the calling thread, whose `WM_TIMER` carries no window. The message loop
/// ([`run_message_loop`]) reports it as [`ThreadMessage::Timer`] with the `id` it was started with.
pub struct ThreadTimer(usize);

impl ThreadTimer {
    pub fn start(id: usize, milliseconds: u32) -> Self {
        let given = unsafe { SetTimer(std::ptr::null_mut(), id, milliseconds, None) };
        TIMERS.with(|timers| timers.borrow_mut().push((given, id)));
        Self(given)
    }
}

impl Drop for ThreadTimer {
    fn drop(&mut self) {
        unsafe { KillTimer(std::ptr::null_mut(), self.0) };
        TIMERS.with(|timers| timers.borrow_mut().retain(|(given, _)| *given != self.0));
    }
}

/// A message posted to the thread itself, not to a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadMessage {
    /// A [`ThreadTimer`] ticked; the number is the id it was started with.
    Timer(usize),
    /// A [`crate::Background`] job finished.
    Background,
    /// Any other message, by its code.
    Other(u32),
}

/// Runs the calling thread's message loop until `stop` says so (checked before each wait) or the
/// loop ends. Messages for a window are dispatched; a message posted to the thread itself goes to
/// `on_thread_message`.
pub fn run_message_loop(
    mut stop: impl FnMut() -> bool,
    mut on_thread_message: impl FnMut(ThreadMessage),
) {
    let mut message: MSG = unsafe { std::mem::zeroed() };
    while !stop() && unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        if message.hwnd.is_null() {
            on_thread_message(match message.message {
                WM_TIMER => ThreadMessage::Timer(TIMERS.with(|timers| {
                    timers
                        .borrow()
                        .iter()
                        .find(|(given, _)| *given == message.wParam)
                        .map_or(message.wParam, |(_, asked)| *asked)
                })),
                crate::WM_BACKGROUND => ThreadMessage::Background,
                other => ThreadMessage::Other(other),
            });
            continue;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: a name WebView2 made unique is given back as the page named it.
    #[test]
    fn dc_0f1k_11_2__a_numbered_or_stamped_download_name_is_the_pages_own() {
        let name = |p: &str| {
            proposed_name(Path::new(p))
                .unwrap()
                .to_string_lossy()
                .into_owned()
        };
        assert_eq!(
            name(r"C:\d\Book1 - 2026-11-09T011151.319.xlsx"),
            "Book1.xlsx"
        );
        assert_eq!(name(r"C:\d\Book1.xlsx"), "Book1.xlsx");
        assert_eq!(name(r"C:\d\a - b.xlsx"), "a - b.xlsx");
        assert_eq!(name(r"C:\d\Book (1).xlsx"), "Book (1).xlsx");
    }

    /// Native-only: the carrier's base64 must agree with the page's atob/btoa framing.
    #[test]
    fn bytes_cross_the_bridge_exactly() {
        for length in [0usize, 1, 2, 3, 4, 5, 255, 256, 1000] {
            let bytes: Vec<u8> = (0..length).map(|i| (i * 7 + 3) as u8).collect();
            let text = base64(&bytes);
            assert_eq!(
                unbase64(&text).as_deref(),
                Some(bytes.as_slice()),
                "{length}"
            );
        }
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(unbase64("TWE="), Some(b"Ma".to_vec()));
        assert_eq!(unbase64("T*E="), None);
    }

    /// Native-only: a message splits at the first separator into its text and decoded bytes.
    #[test]
    fn messages_split_into_text_and_bytes() {
        assert_eq!(split_message("hello"), ("hello", None));
        let framed = format!("tiles{SEPARATOR}TWE=");
        assert_eq!(split_message(&framed), ("tiles", Some(b"Ma".to_vec())));
    }
}
