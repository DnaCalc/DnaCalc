// SPDX-License-Identifier: MIT
// Measured Excel 16.0.20527 x64 dual-interface metadata (REGKIND_NONE).
#pragma once
#include <windows.h>
#include <oaidl.h>
namespace dna_rtd {
inline constexpr GUID update_iid={0xa43788c1,0xd91b,0x11d3,{0x8f,0x39,0,0xc0,0x4f,0x36,0x51,0xb8}};
inline constexpr GUID server_iid={0xec0e6191,0xdb51,0x11d3,{0x8f,0x3e,0,0xc0,0x4f,0x36,0x51,0xb8}};
struct Update : IDispatch {
    virtual HRESULT STDMETHODCALLTYPE UpdateNotify()=0;
    virtual HRESULT STDMETHODCALLTYPE get_HeartbeatInterval(LONG*)=0;
    virtual HRESULT STDMETHODCALLTYPE put_HeartbeatInterval(LONG)=0;
    virtual HRESULT STDMETHODCALLTYPE Disconnect()=0;
};
struct Rtd : IDispatch {
    virtual HRESULT STDMETHODCALLTYPE ServerStart(Update*,LONG*)=0;
    virtual HRESULT STDMETHODCALLTYPE ConnectData(LONG,SAFEARRAY**,VARIANT_BOOL*,VARIANT*)=0;
    virtual HRESULT STDMETHODCALLTYPE RefreshData(LONG*,SAFEARRAY**)=0;
    virtual HRESULT STDMETHODCALLTYPE DisconnectData(LONG)=0;
    virtual HRESULT STDMETHODCALLTYPE Heartbeat(LONG*)=0;
    virtual HRESULT STDMETHODCALLTYPE ServerTerminate()=0;
};
}
