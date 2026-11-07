//! The desktop's one-GET transport for WEBSERVICE and IMAGE (WinHTTP). It carries no rules: the
//! redirect, status and body rules are `calc_func::fetch_following`'s, which every host shares.
//! A request here sends no `Accept-Encoding`, follows no redirect and decompresses nothing, and
//! has no cross-origin restriction (the native worker is not a page).
use calc_func::RawResponse;
use std::{ffi::c_void, ptr::null, time::Duration, time::Instant};
use windows_sys::Win32::Networking::WinHttp::*;

/// The time a connect, send or receive may take, and the whole body's.
const STEP: i32 = 6_000;
const WHOLE: Duration = Duration::from_secs(8);
/// The most body bytes read.
const MOST_BODY: usize = 64 * 1024 * 1024;

struct Handle(*mut c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// (secure, host, port, path and query) of an `http` or `https` URL.
fn split(url: &str) -> Option<(bool, String, u16, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let secure = match scheme.to_ascii_lowercase().as_str() {
        "https" => true,
        "http" => false,
        _ => return None,
    };
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let path = if tail.starts_with('/') { tail.to_owned() } else { format!("/{tail}") };
    let default = if secure { 443 } else { 80 };
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let (host, after) = inner.split_once(']')?;
        match after.strip_prefix(':') {
            Some(port) => (host, port.parse().ok()?),
            None => (host, default),
        }
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host, port.parse().ok()?),
            None => (authority, default),
        }
    };
    (!host.is_empty()).then(|| (secure, host.to_owned(), port, path))
}

/// One GET of `url`, or `None` when no response came (connection, name resolution, TLS, timeout).
pub fn get(url: &str) -> Option<RawResponse> {
    let (secure, host, port, path) = split(url)?;
    unsafe {
        let session = Handle(WinHttpOpen(
            wide("DNA Calc").as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        ));
        if session.0.is_null() {
            return None;
        }
        WinHttpSetTimeouts(session.0, STEP, STEP, STEP, STEP);
        let connection = Handle(WinHttpConnect(session.0, wide(&host).as_ptr(), port, 0));
        if connection.0.is_null() {
            return None;
        }
        let request = Handle(WinHttpOpenRequest(
            connection.0,
            wide("GET").as_ptr(),
            wide(&path).as_ptr(),
            null(),
            null(),
            std::ptr::null_mut(),
            if secure { WINHTTP_FLAG_SECURE } else { 0 },
        ));
        if request.0.is_null() {
            return None;
        }
        let disable = WINHTTP_DISABLE_REDIRECTS;
        WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_DISABLE_FEATURE,
            (&disable as *const u32).cast(),
            4,
        );
        let started = Instant::now();
        if WinHttpSendRequest(request.0, null(), 0, null(), 0, 0, 0) == 0
            || WinHttpReceiveResponse(request.0, std::ptr::null_mut()) == 0
        {
            return None;
        }
        let mut status = 0u32;
        let mut size = 4u32;
        if WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&mut status as *mut u32).cast(),
            &mut size,
            std::ptr::null_mut(),
        ) == 0
        {
            return None;
        }
        let mut location = vec![0u16; 4096];
        let mut size = (location.len() * 2) as u32;
        let location = (WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_LOCATION,
            null(),
            location.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
        ) != 0)
            .then(|| String::from_utf16_lossy(&location[..size as usize / 2]));
        let mut body = Vec::new();
        loop {
            let mut available = 0u32;
            if WinHttpQueryDataAvailable(request.0, &mut available) == 0 {
                return None;
            }
            if available == 0 {
                break;
            }
            let at = body.len();
            body.resize(at + available as usize, 0u8);
            let mut read = 0u32;
            if WinHttpReadData(request.0, body[at..].as_mut_ptr().cast(), available, &mut read) == 0 {
                return None;
            }
            body.truncate(at + read as usize);
            if body.len() > MOST_BODY || started.elapsed() > WHOLE {
                return None;
            }
        }
        Some(RawResponse {
            status: status as u16,
            location,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: pure URL splitting for the WinHTTP transport.
    #[test]
    fn urls_split_into_host_port_and_path() {
        let split = |url| split(url).map(|(s, h, p, t)| (s, h, p, t));
        let owned = |s: bool, h: &str, p: u16, t: &str| Some((s, h.to_owned(), p, t.to_owned()));
        assert_eq!(split("http://a.test/x?y=1"), owned(false, "a.test", 80, "/x?y=1"));
        assert_eq!(split("https://a.test:8443"), owned(true, "a.test", 8443, "/"));
        assert_eq!(split("http://a.test?q"), owned(false, "a.test", 80, "/?q"));
        assert_eq!(split("http://[::1]:81/p"), owned(false, "::1", 81, "/p"));
        assert_eq!(split("ftp://a.test/"), None);
    }
}
