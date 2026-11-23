use super::{AddinRegistry, Registration, RegistrationScope};
use std::path::PathBuf;
use windows::{
    Win32::{
        Foundation::{
            ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS,
        },
        System::{Com::CLSIDFromString, Registry::*},
    },
    core::{GUID, PCWSTR, PWSTR, w},
};

const OFFICE_ADDINS: PCWSTR = w!("Software\\Microsoft\\Office\\Excel\\Addins");

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

pub(crate) struct Registry;
impl AddinRegistry for Registry {
    fn registrations(&self, scope: RegistrationScope) -> Result<Vec<Registration>, i32> {
        let root = match scope {
            RegistrationScope::Machine => HKEY_LOCAL_MACHINE,
            RegistrationScope::User => HKEY_CURRENT_USER,
        };
        let mut key = HKEY::default();
        let code = unsafe { RegOpenKeyExW(root, OFFICE_ADDINS, None, KEY_READ, &mut key) };
        if code == ERROR_FILE_NOT_FOUND || code == ERROR_PATH_NOT_FOUND {
            return Ok(Vec::new());
        }
        code.ok().map_err(|error| error.code().0)?;
        let key = Key(key);
        let mut registrations = Vec::new();
        for index in 0.. {
            let mut buffer = [0u16; 256];
            let mut length = buffer.len() as u32;
            let code = unsafe {
                RegEnumKeyExW(
                    key.0,
                    index,
                    Some(PWSTR(buffer.as_mut_ptr())),
                    &mut length,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if code == ERROR_NO_MORE_ITEMS {
                break;
            }
            code.ok().map_err(|error| error.code().0)?;
            let progid = String::from_utf16_lossy(&buffer[..length as usize]);
            let load_behavior = registry_dword(key.0, &progid, w!("LoadBehavior")).unwrap_or(0);
            let friendly_name = registry_string(key.0, &progid, Some("FriendlyName"))
                .unwrap_or_else(|| progid.clone());
            let description =
                registry_string(key.0, &progid, Some("Description")).unwrap_or_default();
            registrations.push(Registration {
                progid,
                friendly_name,
                description,
                load_behavior,
            });
        }
        Ok(registrations)
    }
}

#[derive(Clone)]
pub(crate) enum Server {
    InProcess(PathBuf),
    Local,
}

pub(crate) fn server(progid: &str) -> Option<(GUID, Server)> {
    let clsid = registry_string(HKEY_CLASSES_ROOT, &format!("{progid}\\CLSID"), None)?;
    let clsid_wide = wide(&clsid);
    let id = unsafe { CLSIDFromString(PCWSTR(clsid_wide.as_ptr())) }.ok()?;
    let key = format!("CLSID\\{clsid}");
    if let Some(module) =
        registry_string(HKEY_CLASSES_ROOT, &format!("{key}\\InprocServer32"), None)
            .filter(|module| !module.is_empty())
    {
        return Some((id, Server::InProcess(PathBuf::from(module))));
    }
    registry_string(HKEY_CLASSES_ROOT, &format!("{key}\\LocalServer32"), None)
        .filter(|command| !command.is_empty())
        .map(|_| (id, Server::Local))
}

fn registry_dword(root: HKEY, key: &str, name: PCWSTR) -> Option<u32> {
    let key = wide(key);
    let mut value = 0u32;
    let mut bytes = size_of::<u32>() as u32;
    let code = unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            name,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut bytes),
        )
    };
    (code == ERROR_SUCCESS && bytes as usize == size_of::<u32>()).then_some(value)
}

pub(crate) fn registry_string(root: HKEY, key: &str, name: Option<&str>) -> Option<String> {
    let key = wide(key);
    let name = name.map(wide);
    let name = name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
    let mut bytes = 0;
    if unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            name,
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            None,
            None,
            Some(&mut bytes),
        )
    } != ERROR_SUCCESS
        || bytes < 2
        || bytes > 65536
        || bytes % 2 != 0
    {
        return None;
    }
    let mut buffer = vec![0u16; bytes as usize / 2];
    if unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            name,
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    } != ERROR_SUCCESS
        || bytes % 2 != 0
        || bytes as usize > buffer.len() * 2
    {
        return None;
    }
    let buffer = &buffer[..bytes as usize / 2];
    let end = buffer.iter().position(|c| *c == 0)?;
    if buffer[end..].iter().any(|c| *c != 0) {
        return None;
    }
    String::from_utf16(&buffer[..end]).ok()
}
