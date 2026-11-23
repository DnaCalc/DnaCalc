// Win64 standalone COM add-in fixture. See the Windows replay steps for private registration.
#include <windows.h>
#include <oaidl.h>
#include <cstdio>
#include <cwchar>
#include <new>

struct __declspec(uuid("B65AD801-ABAF-11D0-BB8B-00A0C90F2744")) Addin : IDispatch {
    virtual HRESULT STDMETHODCALLTYPE OnConnection(IDispatch*, LONG, IDispatch*, SAFEARRAY**) = 0;
    virtual HRESULT STDMETHODCALLTYPE OnDisconnection(LONG, SAFEARRAY**) = 0;
    virtual HRESULT STDMETHODCALLTYPE OnAddInsUpdate(SAFEARRAY**) = 0;
    virtual HRESULT STDMETHODCALLTYPE OnStartupComplete(SAFEARRAY**) = 0;
    virtual HRESULT STDMETHODCALLTYPE OnBeginShutdown(SAFEARRAY**) = 0;
};

struct __declspec(uuid("000C0396-0000-0000-C000-000000000046")) Ribbon : IDispatch {
    virtual HRESULT STDMETHODCALLTYPE GetCustomUI(BSTR, BSTR*) = 0;
};

static const CLSID fixture = {0xcad10137, 0xa8c9, 0x4535, {0x94, 0x74, 0x6c, 0x52, 0x1f, 0xe6, 0x6e, 0x03}};

static void trace(const char* event, long result = 0) {
    wchar_t path[MAX_PATH];
    DWORD length = GetEnvironmentVariableW(L"DNACALC_COM_ADDIN_TRACE", path, MAX_PATH);
#ifdef CAD1_LOCAL
    if (!length) {
        length = GetModuleFileNameW(nullptr, path, MAX_PATH);
        if (!length || length + 7 >= MAX_PATH) return;
        wcscat_s(path, L".trace");
    }
#endif
    if (!length || length >= MAX_PATH) return;
    if (FILE* file = _wfopen(path, L"a")) {
        std::fprintf(file, "%s %ld\n", event, result);
        std::fclose(file);
    }
}

static HRESULT call(IDispatch* object, const wchar_t* name, WORD flags,
                    VARIANT* arg = nullptr, VARIANT* result = nullptr) {
    LPOLESTR member = const_cast<LPOLESTR>(name);
    DISPID id, put = DISPID_PROPERTYPUT;
    HRESULT hr = object->GetIDsOfNames(IID_NULL, &member, 1, 0, &id);
    if (FAILED(hr)) return hr;
    DISPPARAMS params = {arg, flags == DISPATCH_PROPERTYPUT ? &put : nullptr,
                         arg ? 1u : 0u, flags == DISPATCH_PROPERTYPUT ? 1u : 0u};
    return object->Invoke(id, IID_NULL, 0, flags, &params, result, nullptr, nullptr);
}

class Provider final : public Addin, public Ribbon {
    LONG refs = 1;
    IDispatch* application = nullptr;
    IDispatch* ui = nullptr;
    bool pressed = false;

    HRESULT write(long value) {
        VARIANT sheet{}, address{}, range{}, number{};
        HRESULT hr = call(application, L"ActiveSheet", DISPATCH_PROPERTYGET, nullptr, &sheet);
        if (SUCCEEDED(hr) && sheet.vt != VT_DISPATCH) hr = DISP_E_TYPEMISMATCH;
        if (SUCCEEDED(hr)) {
            address.vt = VT_BSTR;
            address.bstrVal = SysAllocString(L"A1");
            hr = address.bstrVal ? call(sheet.pdispVal, L"Range", DISPATCH_PROPERTYGET, &address, &range)
                                 : E_OUTOFMEMORY;
        }
        if (SUCCEEDED(hr) && range.vt != VT_DISPATCH) hr = DISP_E_TYPEMISMATCH;
        if (SUCCEEDED(hr)) {
            number.vt = VT_I4;
            number.lVal = value;
            hr = call(range.pdispVal, L"Value2", DISPATCH_PROPERTYPUT, &number);
        }
        VariantClear(&range);
        VariantClear(&address);
        VariantClear(&sheet);
        return hr;
    }

    void release_objects() {
        if (ui) ui->Release();
        if (application) application->Release();
        ui = application = nullptr;
    }

public:
    ~Provider() { release_objects(); }

    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void** result) override {
        if (!result) return E_POINTER;
        *result = nullptr;
        if (iid == IID_IUnknown || iid == IID_IDispatch || iid == __uuidof(Addin))
            *result = static_cast<Addin*>(this);
        else if (iid == __uuidof(Ribbon))
            *result = static_cast<Ribbon*>(this);
        else
            return E_NOINTERFACE;
        AddRef();
        return S_OK;
    }

    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&refs); }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG count = InterlockedDecrement(&refs);
        if (!count) delete this;
        return count;
    }

    HRESULT STDMETHODCALLTYPE GetTypeInfoCount(UINT* count) override { *count = 0; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetTypeInfo(UINT, LCID, ITypeInfo**) override { return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE GetIDsOfNames(REFIID, LPOLESTR* names, UINT count, LCID, DISPID* ids) override {
        const wchar_t* callbacks[] = {L"Load", L"Click", L"Label", L"Enabled", L"Visible"};
        if (count != 1) return DISP_E_UNKNOWNNAME;
        for (DISPID i = 0; i < 5; ++i) {
            if (wcscmp(names[0], callbacks[i]) == 0) { ids[0] = i + 1; return S_OK; }
        }
        return DISP_E_UNKNOWNNAME;
    }

    HRESULT STDMETHODCALLTYPE Invoke(DISPID id, REFIID, LCID, WORD, DISPPARAMS* params,
                                    VARIANT* result, EXCEPINFO*, UINT*) override {
        if (!params || params->cArgs != 1 || params->rgvarg[0].vt != VT_DISPATCH)
            return DISP_E_TYPEMISMATCH;
        IDispatch* object = params->rgvarg[0].pdispVal;
        if (id == 1) {
            if (ui) ui->Release();
            ui = object;
            ui->AddRef();
            trace("onLoad");
            return S_OK;
        }
        if (id == 2) {
            VARIANT control_id{};
            HRESULT hr = call(object, L"Id", DISPATCH_PROPERTYGET, nullptr, &control_id);
            if (SUCCEEDED(hr) && (control_id.vt != VT_BSTR || wcscmp(control_id.bstrVal, L"cad1") != 0))
                hr = DISP_E_TYPEMISMATCH;
            VariantClear(&control_id);
            if (SUCCEEDED(hr)) hr = write(72);
            if (SUCCEEDED(hr)) {
                pressed = true;
                VARIANT name{};
                name.vt = VT_BSTR;
                name.bstrVal = SysAllocString(L"cad1");
                hr = name.bstrVal ? call(ui, L"InvalidateControl", DISPATCH_METHOD, &name)
                                  : E_OUTOFMEMORY;
                VariantClear(&name);
            }
            trace("onAction", hr);
            return hr;
        }
        if (!result) return E_POINTER;
        if (id == 3) {
            result->vt = VT_BSTR;
            result->bstrVal = SysAllocString(pressed ? L"Clicked" : L"Run COM sample");
            trace("getLabel");
            return result->bstrVal ? S_OK : E_OUTOFMEMORY;
        }
        if (id == 4 || id == 5) {
            result->vt = VT_BOOL;
            result->boolVal = VARIANT_TRUE;
            trace(id == 4 ? "getEnabled" : "getVisible");
            return S_OK;
        }
        return DISP_E_MEMBERNOTFOUND;
    }

    HRESULT STDMETHODCALLTYPE OnConnection(IDispatch* app, LONG mode, IDispatch*, SAFEARRAY**) override {
        release_objects();
        trace("OnConnection", mode);
        application = app;
        application->AddRef();
        HRESULT hr = write(71);
        trace("StartupWrite", hr);
        return hr;
    }

    HRESULT STDMETHODCALLTYPE OnDisconnection(LONG mode, SAFEARRAY**) override {
        trace("OnDisconnection", mode);
        release_objects();
#ifdef CAD1_LOCAL
        PostQuitMessage(0);
#endif
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE OnAddInsUpdate(SAFEARRAY**) override { trace("OnAddInsUpdate"); return S_OK; }
    HRESULT STDMETHODCALLTYPE OnStartupComplete(SAFEARRAY**) override { trace("OnStartupComplete"); return S_OK; }
    HRESULT STDMETHODCALLTYPE OnBeginShutdown(SAFEARRAY**) override { trace("OnBeginShutdown"); return S_OK; }

    HRESULT STDMETHODCALLTYPE GetCustomUI(BSTR id, BSTR* result) override {
        if (wcscmp(id, L"Microsoft.Excel.Workbook") != 0) return E_INVALIDARG;
        trace("GetCustomUI");
        *result = SysAllocString(
            L"<customUI xmlns='http://schemas.microsoft.com/office/2009/07/customui' onLoad='Load'>"
            L"<ribbon><tabs><tab id='Cad1Tab' label='COM sample'><group id='Cad1Group' label='Fixture'>"
            L"<button id='cad1' getLabel='Label' getEnabled='Enabled' getVisible='Visible' onAction='Click'/>"
            L"</group></tab></tabs></ribbon></customUI>");
        return *result ? S_OK : E_OUTOFMEMORY;
    }
};

class Factory final : public IClassFactory {
    LONG refs = 1;
public:
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void** result) override {
        if (!result) return E_POINTER;
        *result = nullptr;
        if (iid != IID_IUnknown && iid != IID_IClassFactory) return E_NOINTERFACE;
        *result = this;
        AddRef();
        return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&refs); }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG count = InterlockedDecrement(&refs);
        if (!count) delete this;
        return count;
    }
    HRESULT STDMETHODCALLTYPE CreateInstance(IUnknown* outer, REFIID iid, void** result) override {
        if (outer) return CLASS_E_NOAGGREGATION;
        Provider* provider = new (std::nothrow) Provider;
        if (!provider) return E_OUTOFMEMORY;
        HRESULT hr = provider->QueryInterface(iid, result);
        provider->Release();
        return hr;
    }
    HRESULT STDMETHODCALLTYPE LockServer(BOOL) override { return S_OK; }
};

#ifndef CAD1_LOCAL
HRESULT WINAPI DllGetClassObject(REFCLSID clsid, REFIID iid, void** result) {
    if (clsid != fixture) return CLASS_E_CLASSNOTAVAILABLE;
    Factory* factory = new (std::nothrow) Factory;
    if (!factory) return E_OUTOFMEMORY;
    HRESULT hr = factory->QueryInterface(iid, result);
    factory->Release();
    return hr;
}
#else
int wmain() {
    if (FAILED(CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED))) return 1;
    Factory* factory = new (std::nothrow) Factory;
    if (!factory) { CoUninitialize(); return 1; }
    DWORD cookie = 0;
    HRESULT hr = CoRegisterClassObject(fixture, factory, CLSCTX_LOCAL_SERVER, REGCLS_SINGLEUSE, &cookie);
    factory->Release();
    if (SUCCEEDED(hr)) {
        MSG message;
        while (GetMessageW(&message, nullptr, 0, 0) > 0) {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        CoRevokeClassObject(cookie);
    }
    CoUninitialize();
    return FAILED(hr) ? 1 : 0;
}
#endif
