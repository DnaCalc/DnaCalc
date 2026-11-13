//! Foreign interfaces called by the host. Signatures are Excel/Office descriptor ABI facts.
#![allow(non_snake_case)]
use std::ffi::c_void;
use windows::{
    Win32::System::{
        Com::{IDispatch, IDispatch_Impl, IDispatch_Vtbl, SAFEARRAY},
        Variant::VARIANT,
    },
    core::{HRESULT, interface},
};

#[interface("B65AD801-ABAF-11D0-BB8B-00A0C90F2744")]
pub(crate) unsafe trait IDTExtensibility2: IDispatch {
    pub(crate) fn OnConnection(
        &self,
        application: *mut c_void,
        mode: i32,
        addin: *mut c_void,
        custom: *mut *mut SAFEARRAY,
    ) -> HRESULT;
    pub(crate) fn OnDisconnection(&self, mode: i32, custom: *mut *mut SAFEARRAY) -> HRESULT;
    pub(crate) fn OnAddInsUpdate(&self, custom: *mut *mut SAFEARRAY) -> HRESULT;
    pub(crate) fn OnStartupComplete(&self, custom: *mut *mut SAFEARRAY) -> HRESULT;
    pub(crate) fn OnBeginShutdown(&self, custom: *mut *mut SAFEARRAY) -> HRESULT;
}
#[interface("000C0396-0000-0000-C000-000000000046")]
pub(crate) unsafe trait IRibbonExtensibility: IDispatch {
    pub(crate) fn GetCustomUI(&self, ribbon: *mut c_void, result: *mut *mut c_void) -> HRESULT;
}
#[interface("EC0E6191-DB51-11D3-8F3E-00C04F3651B8")]
pub(crate) unsafe trait IRtdServer: IDispatch {
    pub(crate) fn ServerStart(&self, callback: *mut c_void, result: *mut i32) -> HRESULT;
    pub(crate) fn ConnectData(
        &self,
        topic: i32,
        strings: *mut *mut SAFEARRAY,
        fresh: *mut i16,
        result: *mut VARIANT,
    ) -> HRESULT;
    pub(crate) fn RefreshData(&self, count: *mut i32, result: *mut *mut SAFEARRAY) -> HRESULT;
    pub(crate) fn DisconnectData(&self, topic: i32) -> HRESULT;
    pub(crate) fn Heartbeat(&self, result: *mut i32) -> HRESULT;
    pub(crate) fn ServerTerminate(&self) -> HRESULT;
}
#[interface("A43788C1-D91B-11D3-8F39-00C04F3651B8")]
pub(crate) unsafe trait IRTDUpdateEvent: IDispatch {
    pub(crate) fn UpdateNotify(&self) -> HRESULT;
    pub(crate) fn HeartbeatInterval(&self, result: *mut i32) -> HRESULT;
    pub(crate) fn SetHeartbeatInterval(&self, value: i32) -> HRESULT;
    pub(crate) fn Disconnect(&self) -> HRESULT;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{guid, id};
    use calc_object_model::Descriptor;
    use windows::core::Interface;

    /// Native-only: checks the exact native foreign-interface vtables and IIDs against the descriptor.
    #[test]
    pub(crate) fn foreign_iids_and_every_slot_match_the_descriptor() {
        macro_rules! check {
            ($interface:ty, $name:literal, $(($field:ident, $member:literal, $slot:literal)),+ $(,)?) => {{
                let interface = id($name);
                assert_eq!(<$interface>::IID, guid(interface));
                assert_eq!(size_of::<<$interface as Interface>::Vtable>() / size_of::<usize>(), Descriptor::get().ty(interface).vtable_slots.unwrap() as usize);
                $(assert_eq!(std::mem::offset_of!(<$interface as Interface>::Vtable, $field) / size_of::<usize>(), $slot);
                  assert_eq!(Descriptor::get().member_by_slot(interface, $slot).unwrap().name, $member);)+
            }};
        }
        check!(
            IDTExtensibility2,
            "_IDTExtensibility2",
            (OnConnection, "OnConnection", 7),
            (OnDisconnection, "OnDisconnection", 8),
            (OnAddInsUpdate, "OnAddInsUpdate", 9),
            (OnStartupComplete, "OnStartupComplete", 10),
            (OnBeginShutdown, "OnBeginShutdown", 11)
        );
        check!(
            IRibbonExtensibility,
            "IRibbonExtensibility",
            (GetCustomUI, "GetCustomUI", 7)
        );
        check!(
            IRtdServer,
            "IRtdServer",
            (ServerStart, "ServerStart", 7),
            (ConnectData, "ConnectData", 8),
            (RefreshData, "RefreshData", 9),
            (DisconnectData, "DisconnectData", 10),
            (Heartbeat, "Heartbeat", 11),
            (ServerTerminate, "ServerTerminate", 12)
        );
        check!(
            IRTDUpdateEvent,
            "IRTDUpdateEvent",
            (UpdateNotify, "UpdateNotify", 7),
            (HeartbeatInterval, "HeartbeatInterval", 8),
            (SetHeartbeatInterval, "HeartbeatInterval", 9),
            (Disconnect, "Disconnect", 10)
        );
    }
}
