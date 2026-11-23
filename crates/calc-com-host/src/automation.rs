//! Publishing the desktop's existing STA objects for local Automation clients.
use crate::{projection::Projection, values::Objects};
use calc_object_model::AutoValue;
use std::{
    ffi::c_void,
    os::windows::ffi::OsStrExt,
    path::{Component, Path, PathBuf, Prefix},
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::{CLASS_E_NOAGGREGATION, E_POINTER},
        System::{Com::*, Ole::*},
    },
    core::{BOOL, Error, GUID, IUnknown, Interface, PCWSTR, Ref, implement},
};

/// The CLSID of DnaCalc.Application, independent of the Excel coclass in the descriptor.
pub const APPLICATION_CLSID: GUID = GUID::from_u128(0xb363db86_b412_4c60_91f2_9d538f75bb65);

#[implement(IClassFactory, Agile = false)]
struct Factory(IDispatch);

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<'_, IUnknown>,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if out.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        unsafe { out.write(std::ptr::null_mut()) };
        if !outer.is_null() {
            return Err(Error::from_hresult(CLASS_E_NOAGGREGATION));
        }
        if iid.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        unsafe { self.0.query(iid, out) }.ok()
    }

    fn LockServer(&self, _: BOOL) -> windows::core::Result<()> {
        // The visible desktop's lifetime is controlled by the window.
        Ok(())
    }
}

struct ClassRegistration(u32);
impl Drop for ClassRegistration {
    fn drop(&mut self) {
        let _ = unsafe { CoRevokeClassObject(self.0) };
    }
}

struct ActiveRegistration(u32);
impl Drop for ActiveRegistration {
    fn drop(&mut self) {
        let _ = unsafe { RevokeActiveObject(self.0, None) };
    }
}

struct WorkbookRegistration {
    rot: IRunningObjectTable,
    cookie: u32,
    workbook: u64,
    path: PathBuf,
}
impl Drop for WorkbookRegistration {
    fn drop(&mut self) {
        let _ = unsafe { self.rot.Revoke(self.cookie) };
    }
}

/// Owns local COM registrations on the desktop STA. Dropping it revokes them before the
/// projection and its workbook context shut down.
pub struct LocalAutomation {
    _class: ClassRegistration,
    workbook: Option<WorkbookRegistration>,
    _active: ActiveRegistration,
    projection: Rc<Projection>,
}

impl LocalAutomation {
    pub fn publish(projection: Rc<Projection>) -> Result<Self, String> {
        let app = projection.application()?;
        let mut active = 0;
        unsafe { RegisterActiveObject(&app, &APPLICATION_CLSID, ACTIVEOBJECT_WEAK, &mut active) }
            .map_err(|e| format!("RegisterActiveObject: {e}"))?;
        let active = ActiveRegistration(active);
        let factory: IClassFactory = Factory(app).into();
        let class = unsafe {
            CoRegisterClassObject(
                &APPLICATION_CLSID,
                &factory,
                CLSCTX_LOCAL_SERVER,
                REGCLS_MULTIPLEUSE,
            )
        }
        .map_err(|e| format!("CoRegisterClassObject: {e}"))?;
        Ok(Self {
            _class: ClassRegistration(class),
            workbook: None,
            _active: active,
            projection,
        })
    }

    /// Change the file moniker after a successful open or save. Unsaved books and paths
    /// outside a local drive have no published file moniker.
    pub fn set_workbook(&mut self, workbook: u64, path: &Path) -> Result<(), String> {
        if self
            .workbook
            .as_ref()
            .is_some_and(|entry| entry.workbook == workbook && entry.path == path)
        {
            return Ok(());
        }
        drop(self.workbook.take());
        if !path.is_absolute()
            || !matches!(path.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)))
        {
            return Ok(());
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let rot = unsafe { GetRunningObjectTable(0) }.map_err(|e| e.to_string())?;
        let moniker =
            unsafe { CreateFileMoniker(PCWSTR(wide.as_ptr())) }.map_err(|e| e.to_string())?;
        let state = &self.projection.state;
        let value = self
            .projection
            .member(&state.application(), "ActiveWorkbook", vec![])
            .map_err(|e| e.description())?;
        let objects = state.result_values(std::slice::from_ref(&value));
        let AutoValue::Object(book) = &value else {
            return Err("ActiveWorkbook returned no workbook".into());
        };
        let book = objects.wrap(book).map_err(|e| e.description())?;
        let cookie = unsafe { rot.Register(ROTFLAGS_REGISTRATIONKEEPSALIVE, &book, &moniker) }
            .map_err(|e| e.to_string())?;
        self.workbook = Some(WorkbookRegistration {
            rot,
            cookie,
            workbook,
            path: path.into(),
        });
        Ok(())
    }
}
