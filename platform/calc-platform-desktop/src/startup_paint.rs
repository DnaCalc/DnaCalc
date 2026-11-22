//! Compare a local WebView2 test page with the pixels on the interactive desktop.

use crate::shell::{client_size, pump_pending};
use std::{
    cell::{Cell, RefCell},
    ptr::null_mut,
    rc::Rc,
    time::{Duration, Instant},
};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use webview2_com::{CoTaskMemPWSTR, WebMessageReceivedEventHandler, take_pwstr};
use windows::core::PWSTR;
use windows_sys::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::{CLR_INVALID, ClientToScreen, GetDC, GetPixel, ReleaseDC},
    System::{
        StationsAndDesktops::{GetThreadDesktop, GetUserObjectInformationW, UOI_IO},
        Threading::GetCurrentThreadId,
    },
    UI::WindowsAndMessaging::{IsChild, IsWindow, IsWindowVisible, WindowFromPoint},
};

const PAINT_TIMEOUT: Duration = Duration::from_secs(2);
const PROBE_LIMIT: Duration = Duration::from_secs(4);

// The two markers correspond to the screen samples. The native bridge is installed after startup.
const PAGE: &str = r#"<!doctype html>
<meta charset="utf-8">
<style>
html { forced-color-adjust: none; background: white; }
html::before, html::after {
  content: ''; position: fixed; top: 46%; width: 8%; height: 8%;
}
html::before { left: 21%; background: rgb(234,234,234); }
html::after { left: 71%; background: rgb(74,94,118); }
</style>
<script>
chrome.webview.postMessage('ready');
try {
  const gl = document.createElement('canvas').getContext('webgl');
  const info = gl?.getExtension('WEBGL_debug_renderer_info');
  const gpu = gl && gl.getParameter(info ? info.UNMASKED_RENDERER_WEBGL : gl.RENDERER);
  chrome.webview.postMessage('gpu:' + (gpu || 'unavailable'));
} catch {
  chrome.webview.postMessage('gpu:unavailable');
}
</script>"#;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Paint {
    Presented,
    Missing,
    Unobservable,
    RendererUnconfirmed,
    WindowClosed,
}

pub(super) fn check(
    view: &ICoreWebView2,
    parent: HWND,
    sync_parent: impl Fn(),
) -> Result<Paint, String> {
    let ready = Rc::new(Cell::new(false));
    let gpu = Rc::new(RefCell::new(None));
    let mut token = 0;
    let handler = WebMessageReceivedEventHandler::create(Box::new({
        let ready = ready.clone();
        let gpu = gpu.clone();
        move |_, args| {
            if let Some(args) = args {
                let mut message = PWSTR::null();
                unsafe { args.TryGetWebMessageAsString(&mut message) }?;
                let message = take_pwstr(message);
                if message == "ready" {
                    ready.set(true);
                } else if let Some(name) = message.strip_prefix("gpu:") {
                    *gpu.borrow_mut() = Some(name.to_owned());
                }
            }
            Ok(())
        }
    }));
    unsafe { view.add_WebMessageReceived(&handler, &mut token) }.map_err(|e| e.to_string())?;
    let html = CoTaskMemPWSTR::from(PAGE);
    let result = unsafe { view.NavigateToString(*html.as_ref().as_pcwstr()) }
        .map(|()| observe(parent, &ready, &sync_parent))
        .map_err(|e| e.to_string());
    let removed = unsafe { view.remove_WebMessageReceived(token) };
    eprintln!(
        "WebView2 startup paint: {result:?}, WebGL renderer: {:?}",
        gpu.borrow().as_deref().unwrap_or("unavailable")
    );
    removed.map_err(|e| e.to_string())?;
    result
}

fn observe(parent: HWND, ready: &Cell<bool>, sync_parent: &impl Fn()) -> Paint {
    let deadline = Instant::now() + PROBE_LIMIT;
    let mut missing_since = None;
    loop {
        pump_pending();
        if unsafe { IsWindow(parent) } == 0 {
            return Paint::WindowClosed;
        }
        sync_parent();
        if ready.get() {
            if !on_input_desktop() {
                return Paint::Unobservable;
            }
            match screen_paint(parent) {
                Some(true) => return Paint::Presented,
                Some(false) => {
                    if missing_since.get_or_insert_with(Instant::now).elapsed() >= PAINT_TIMEOUT {
                        return Paint::Missing;
                    }
                }
                None => missing_since = None,
            }
        }
        if Instant::now() >= deadline {
            return if ready.get() {
                Paint::Unobservable
            } else {
                Paint::RendererUnconfirmed
            };
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn on_input_desktop() -> bool {
    let mut active = 0i32;
    unsafe {
        GetUserObjectInformationW(
            GetThreadDesktop(GetCurrentThreadId()),
            UOI_IO,
            (&raw mut active).cast(),
            std::mem::size_of_val(&active) as u32,
            null_mut(),
        ) != 0
            && active != 0
    }
}

/// None means the pixels cannot be attributed to this view (for example, a covered window).
fn screen_paint(parent: HWND) -> Option<bool> {
    if unsafe { IsWindowVisible(parent) } == 0 {
        return None;
    }
    let (width, height) = client_size(parent)?;
    if width < 32 || height < 32 {
        return None;
    }
    let mut points = [
        POINT {
            x: width / 4,
            y: height / 2,
        },
        POINT {
            x: width - width / 4,
            y: height / 2,
        },
    ];
    for point in &mut points {
        if unsafe { ClientToScreen(parent, point) } == 0 {
            return None;
        }
        let hit = unsafe { WindowFromPoint(*point) };
        if hit != parent && unsafe { IsChild(parent, hit) } == 0 {
            return None;
        }
    }
    unsafe {
        let dc = GetDC(null_mut());
        if dc.is_null() {
            return None;
        }
        let pixels = points.map(|p| GetPixel(dc, p.x, p.y));
        ReleaseDC(null_mut(), dc);
        if pixels.contains(&CLR_INVALID) {
            return None;
        }
        Some(matches_rgb(pixels[0], [234, 234, 234]) && matches_rgb(pixels[1], [74, 94, 118]))
    }
}

fn matches_rgb(color: u32, expected: [u8; 3]) -> bool {
    expected
        .into_iter()
        .enumerate()
        .all(|(index, channel)| ((color >> (index * 8)) as u8).abs_diff(channel) <= 15)
}
