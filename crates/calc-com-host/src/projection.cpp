// SPDX-License-Identifier: MIT
// Interface facts measured from Excel 16.0.20527's type library, not SDK source.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <oleacc.h>
#include <ocidl.h>
#include <olectl.h>
#include <array>
#include <memory>
#include <vector>
#include <string>

namespace {
constexpr GUID application_iid = {0x000208d5,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID window_iid = {0x00020893,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID workbook_iid = {0x000208da,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID workbooks_iid = {0x000208db,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID events_iid = {0x00024413,0,0,{0xc0,0,0,0,0,0,0,0x46}};
struct Value { unsigned kind; double number; const wchar_t* text; size_t length; };
using Run = HRESULT (WINAPI*)(const void*,const wchar_t*,size_t,const Value*,size_t,Value*);
struct Object;
struct State {
    HWND hwnd;
    DWORD thread;
    const void* context;
    Run run;
    bool connected = true;
    bool enable_events = true;
    Object* workbook = nullptr;
    Object* workbooks = nullptr;
    std::wstring workbook_name = L"Book1";
};
struct Object {
    void** vtable;
    volatile LONG references = 1;
    std::shared_ptr<State> state;
    Object* application;
    bool is_application;
    Object* parent = nullptr;
    bool is_workbook = false;
    bool is_collection = false;
};
ULONG WINAPI add_ref(Object* object) { return InterlockedIncrement(&object->references); }
ULONG WINAPI release(Object* object) {
    LONG count = InterlockedDecrement(&object->references);
    if (!count) {
        if (!object->is_application) {
            if(object->parent && object->parent!=object->application) release(object->parent);
            release(object->application);
        }
        delete object;
    }
    return count;
}
HRESULT valid(Object* object) {
    if (!object->state->connected) return CO_E_OBJNOTCONNECTED;
    if (GetCurrentThreadId() != object->state->thread) return RPC_E_WRONG_THREAD;
    return S_OK;
}
HRESULT WINAPI query(Object*, REFIID, void**);

// Connection points retain actual subscriptions. This shell does not yet issue
// workbook edits/events; Advise is not evidence of delivered AppEvents.
class Point final : public IConnectionPoint {
    volatile LONG references = 1;
    Object* owner;
    DWORD next = 1;
    std::vector<std::pair<DWORD,IUnknown*>> sinks;
public:
    explicit Point(Object* object) : owner(object) { add_ref(owner); }
    ~Point() { for (auto& sink:sinks) sink.second->Release(); release(owner); }
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void** out) override {
        if (!out) return E_POINTER;
        *out=nullptr;
        if (iid!=IID_IUnknown && iid!=IID_IConnectionPoint) return E_NOINTERFACE;
        *out=static_cast<IConnectionPoint*>(this); AddRef(); return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&references); }
    ULONG STDMETHODCALLTYPE Release() override { LONG n=InterlockedDecrement(&references); if(!n) delete this; return n; }
    HRESULT STDMETHODCALLTYPE GetConnectionInterface(IID* iid) override { if(!iid) return E_POINTER; *iid=events_iid; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetConnectionPointContainer(IConnectionPointContainer** out) override { return query(owner,IID_IConnectionPointContainer,reinterpret_cast<void**>(out)); }
    HRESULT STDMETHODCALLTYPE Advise(IUnknown* sink,DWORD* cookie) override {
        if(!sink || !cookie) return E_POINTER;
        *cookie=0;
        HRESULT hr=valid(owner); if(FAILED(hr)) return hr;
        IUnknown* events=nullptr;
        hr=sink->QueryInterface(events_iid,reinterpret_cast<void**>(&events));
        if(FAILED(hr)) return CONNECT_E_CANNOTCONNECT;
        DWORD id=next++;
        try { sinks.emplace_back(id,events); } catch (...) { events->Release(); return E_OUTOFMEMORY; }
        *cookie=id; return S_OK;
    }
    HRESULT STDMETHODCALLTYPE Unadvise(DWORD cookie) override {
        for(auto it=sinks.begin();it!=sinks.end();++it) if(it->first==cookie) { it->second->Release(); sinks.erase(it); return S_OK; }
        return CONNECT_E_NOCONNECTION;
    }
    HRESULT STDMETHODCALLTYPE EnumConnections(IEnumConnections** out) override { if(out) *out=nullptr; return E_NOTIMPL; }
};
class Container final : public IConnectionPointContainer {
    volatile LONG references = 1;
    Object* owner;
public:
    explicit Container(Object* object):owner(object) { add_ref(owner); }
    ~Container() { release(owner); }
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void** out) override { return query(owner,iid,out); }
    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&references); }
    ULONG STDMETHODCALLTYPE Release() override { LONG n=InterlockedDecrement(&references); if(!n) delete this; return n; }
    HRESULT STDMETHODCALLTYPE EnumConnectionPoints(IEnumConnectionPoints** out) override { if(out) *out=nullptr; return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE FindConnectionPoint(REFIID iid,IConnectionPoint** out) override {
        if(!out) return E_POINTER;
        *out=nullptr;
        HRESULT hr=valid(owner); if(FAILED(hr)) return hr;
        if(iid!=events_iid) return CONNECT_E_NOCONNECTION;
        auto p=new(std::nothrow) Point(owner); if(!p) return E_OUTOFMEMORY;
        *out=p; return S_OK;
    }
};
HRESULT WINAPI query(Object* object, REFIID iid, void** out) {
    if(!out) return E_POINTER;
    *out=nullptr;
    if(iid==IID_IUnknown || iid==IID_IDispatch ||
       iid==(object->is_application?application_iid:(object->is_workbook?workbook_iid:(object->is_collection?workbooks_iid:window_iid)))) {
        *out=object; add_ref(object); return S_OK;
    }
    if(object->is_application && iid==IID_IConnectionPointContainer) {
        *out=new(std::nothrow) Container(object); return *out?S_OK:E_OUTOFMEMORY;
    }
    return E_NOINTERFACE;
}
HRESULT WINAPI type_count(Object*,UINT* count) { if(!count) return E_POINTER; *count=0; return S_OK; }
HRESULT WINAPI type_info(Object*,UINT,LCID,ITypeInfo** info) { if(info) *info=nullptr; return DISP_E_BADINDEX; }
HRESULT WINAPI names(Object* object,REFIID iid,LPOLESTR* names,UINT count,LCID,DISPID* ids) {
    if(iid!=IID_NULL) return DISP_E_UNKNOWNINTERFACE;
    if(!names || !ids) return E_POINTER;
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    HRESULT result=S_OK;
    for(UINT i=0;i<count;++i) {
        ids[i]=DISPID_UNKNOWN;
        if(!_wcsicmp(names[i],L"Application")) ids[i]=0x94;
        else if(!_wcsicmp(names[i],L"Parent")) ids[i]=0x96;
        else if(!_wcsicmp(names[i],L"Hwnd")) ids[i]=0x79e;
        else if(object->is_application && !_wcsicmp(names[i],L"Version")) ids[i]=0x188;
        else if(object->is_application && !_wcsicmp(names[i],L"Run")) ids[i]=0x103;
        else if((object->is_application || object->is_workbook) && !_wcsicmp(names[i],L"Path")) ids[i]=0x123;
        else if(object->is_application && !_wcsicmp(names[i],L"EnableEvents")) ids[i]=0x4bc;
        else if(object->is_application && !_wcsicmp(names[i],L"Workbooks")) ids[i]=0x23c;
        else if(object->is_workbook && !_wcsicmp(names[i],L"Name")) ids[i]=0x6e;
        else if(object->is_workbook && !_wcsicmp(names[i],L"FullName")) ids[i]=0x121;
        else if(object->is_collection && !_wcsicmp(names[i],L"Count")) ids[i]=0x76;
        else if(object->is_collection && !_wcsicmp(names[i],L"Item")) ids[i]=0xaa;
        else result=DISP_E_UNKNOWNNAME;
    }
    return result;
}
HRESULT get_application(Object* object,IDispatch** out) {
    if(!out) return E_POINTER;
    *out=nullptr;
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    auto app=object->application; add_ref(app); *out=reinterpret_cast<IDispatch*>(app); return S_OK;
}
HRESULT WINAPI get_parent(Object* object,IDispatch** out) {
    if(!out) return E_POINTER;
    *out=nullptr; HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    auto parent=object->parent?object->parent:object->application;
    add_ref(parent); *out=reinterpret_cast<IDispatch*>(parent); return S_OK;
}
HRESULT WINAPI get_path(Object* object,LONG,BSTR* out) {
    if(!out) return E_POINTER;
    *out=nullptr; HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    std::wstring path;
    if(object->is_application) {
        std::array<wchar_t,32768> buffer{};
        auto count=GetModuleFileNameW(nullptr,buffer.data(),static_cast<DWORD>(buffer.size()));
        if(!count || count>=buffer.size()) return HRESULT_FROM_WIN32(GetLastError());
        path.assign(buffer.data(),count); path.resize(path.find_last_of(L"\\/"));
    }
    *out=SysAllocStringLen(path.data(),static_cast<UINT>(path.size()));
    return *out?S_OK:E_OUTOFMEMORY;
}
HRESULT WINAPI get_enable_events(Object* object,VARIANT_BOOL* out) {
    if(!out) return E_POINTER;
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    *out=object->state->enable_events?VARIANT_TRUE:VARIANT_FALSE; return S_OK;
}
HRESULT WINAPI put_enable_events(Object* object,VARIANT_BOOL value) {
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    object->state->enable_events=value!=VARIANT_FALSE; return S_OK;
}
HRESULT WINAPI get_workbooks(Object* object,IDispatch** out) {
    if(!out) return E_POINTER;
    *out=nullptr; HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    add_ref(object->state->workbooks); *out=reinterpret_cast<IDispatch*>(object->state->workbooks); return S_OK;
}
HRESULT WINAPI get_count(Object* object,LONG* out) {
    if(!out) return E_POINTER;
    *out=0; HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    *out=object->state->workbook?1:0; return S_OK;
}
HRESULT WINAPI get_item(Object* object,VARIANT index,IDispatch** out) {
    if(!out) return E_POINTER;
    *out=nullptr; HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    VARIANT value{}; hr=VariantCopyInd(&value,&index); if(FAILED(hr)) return hr;
    bool found=value.vt==VT_BSTR && value.bstrVal && !_wcsicmp(value.bstrVal,object->state->workbook_name.c_str());
    if(value.vt!=VT_BSTR) {
        VARIANT number{}; if(SUCCEEDED(VariantChangeType(&number,&value,0,VT_R8))) found=number.dblVal==1.; VariantClear(&number);
    }
    VariantClear(&value);
    if(!found || !object->state->workbook) return DISP_E_BADINDEX;
    add_ref(object->state->workbook); *out=reinterpret_cast<IDispatch*>(object->state->workbook); return S_OK;
}
HRESULT WINAPI get_name(Object* object,BSTR* out) {
    if(!out) return E_POINTER;
    *out=nullptr; HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    *out=SysAllocString(object->state->workbook_name.c_str()); return *out?S_OK:E_OUTOFMEMORY;
}
HRESULT WINAPI get_fullname(Object* object,LONG,BSTR* out) { return get_name(object,out); }
HRESULT WINAPI get_version(Object* object,LONG,BSTR* out) {
    if(!out) return E_POINTER;
    *out=nullptr;
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    *out=SysAllocString(L"16.0"); return *out?S_OK:E_OUTOFMEMORY;
}
HRESULT WINAPI get_hwnd(Object* object,LONG* out) {
    if(!out) return E_POINTER;
    *out=0;
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    *out=static_cast<LONG>(reinterpret_cast<INT_PTR>(object->state->hwnd)); return S_OK;
}
HRESULT run(Object* object,DISPPARAMS* params,VARIANT* result) {
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    if(params->cNamedArgs) return DISP_E_NONAMEDARGS;
    if(params->cArgs<1 || params->cArgs>31) return DISP_E_BADPARAMCOUNT;
    VARIANT macro; VariantInit(&macro);
    hr=VariantCopyInd(&macro,&params->rgvarg[params->cArgs-1]);
    if(FAILED(hr)) return hr;
    if(macro.vt!=VT_BSTR) { VariantClear(&macro); return DISP_E_TYPEMISMATCH; }
    std::array<VARIANT,30> indirect{};
    std::array<Value,30> values{};
    for(UINT i=0;i+1<params->cArgs;++i) {
        auto& v=indirect[i]; hr=VariantCopyInd(&v,&params->rgvarg[params->cArgs-2-i]);
        if(FAILED(hr)) break;
        auto& value=values[i];
        switch(v.vt) {
            case VT_EMPTY: case VT_NULL: break;
            case VT_ERROR: if(v.scode!=DISP_E_PARAMNOTFOUND) hr=DISP_E_TYPEMISMATCH; break;
            case VT_BSTR: value.kind=2; value.text=v.bstrVal; value.length=SysStringLen(v.bstrVal); break;
            case VT_BOOL: value.kind=3; value.number=v.boolVal!=VARIANT_FALSE; break;
            case VT_R8: value.kind=1; value.number=v.dblVal; break;
            case VT_I4: value.kind=1; value.number=v.lVal; break;
            case VT_I2: value.kind=1; value.number=v.iVal; break;
            default: hr=DISP_E_TYPEMISMATCH;
        }
        if(FAILED(hr)) break;
    }
    if(SUCCEEDED(hr)) {
        size_t count=params->cArgs-1;
        while(count && values[count-1].kind==0) --count;
        Value output{};
        hr=object->state->run(object->state->context,macro.bstrVal,SysStringLen(macro.bstrVal),values.data(),count,&output);
        if(SUCCEEDED(hr) && result) {
            if(output.kind==1) { result->vt=VT_R8; result->dblVal=output.number; }
            else if(output.kind==3) { result->vt=VT_BOOL; result->boolVal=output.number?VARIANT_TRUE:VARIANT_FALSE; }
            else hr=E_NOTIMPL;
        }
    }
    for(auto& v:indirect) VariantClear(&v);
    VariantClear(&macro);
    return hr;
}
HRESULT WINAPI invoke(Object* object,DISPID id,REFIID iid,LCID,WORD flags,DISPPARAMS* params,VARIANT* result,EXCEPINFO*,UINT*) {
    if(result) VariantInit(result);
    if(iid!=IID_NULL) return DISP_E_UNKNOWNINTERFACE;
    if(!params || (params->cArgs && !params->rgvarg)) return E_POINTER;
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    if(object->is_application && id==0x103 && (flags&DISPATCH_METHOD)) return run(object,params,result);
    if(object->is_collection && (id==0xaa || id==0) && (flags&(DISPATCH_METHOD|DISPATCH_PROPERTYGET))) {
        if(params->cArgs!=1) return DISP_E_BADPARAMCOUNT;
        IDispatch* workbook=nullptr; hr=get_item(object,params->rgvarg[0],&workbook);
        if(SUCCEEDED(hr)) { if(result) { result->vt=VT_DISPATCH; result->pdispVal=workbook; } else workbook->Release(); } return hr;
    }
    if(object->is_application && id==0x4bc && (flags&DISPATCH_PROPERTYPUT)) {
        if(params->cArgs!=1) return DISP_E_BADPARAMCOUNT;
        VARIANT value{}; hr=VariantChangeType(&value,&params->rgvarg[0],0,VT_BOOL);
        if(SUCCEEDED(hr)) hr=put_enable_events(object,value.boolVal);
        VariantClear(&value); return hr;
    }
    if(!(flags&DISPATCH_PROPERTYGET)) return DISP_E_MEMBERNOTFOUND;
    if(params->cArgs) return DISP_E_BADPARAMCOUNT;
    if(id==0x94 || id==0x96) {
        IDispatch* app=nullptr; hr=id==0x96?get_parent(object,&app):get_application(object,&app);
        if(SUCCEEDED(hr)) { if(result) { result->vt=VT_DISPATCH; result->pdispVal=app; } else app->Release(); }
        return hr;
    }
    if(id==0x188 && object->is_application) {
        BSTR version=nullptr; hr=get_version(object,0,&version);
        if(SUCCEEDED(hr)) { if(result) { result->vt=VT_BSTR; result->bstrVal=version; } else SysFreeString(version); }
        return hr;
    }
    if(id==0x79e) { LONG hwnd=0; hr=get_hwnd(object,&hwnd); if(SUCCEEDED(hr) && result) { result->vt=VT_I4; result->lVal=hwnd; } return hr; }
    if(id==0x123 && (object->is_application || object->is_workbook)) {
        BSTR path=nullptr; hr=get_path(object,0,&path);
        if(SUCCEEDED(hr)) { if(result) { result->vt=VT_BSTR; result->bstrVal=path; } else SysFreeString(path); }
        return hr;
    }
    if(id==0x4bc && object->is_application) { VARIANT_BOOL value; hr=get_enable_events(object,&value); if(SUCCEEDED(hr) && result) { result->vt=VT_BOOL; result->boolVal=value; } return hr; }
    if(id==0x23c && object->is_application) {
        IDispatch* books=nullptr; hr=get_workbooks(object,&books);
        if(SUCCEEDED(hr)) { if(result) { result->vt=VT_DISPATCH; result->pdispVal=books; } else books->Release(); } return hr;
    }
    if(id==0x76 && object->is_collection) { LONG count; hr=get_count(object,&count); if(SUCCEEDED(hr) && result) { result->vt=VT_I4; result->lVal=count; } return hr; }
    if((id==0x6e || id==0x121) && object->is_workbook) {
        BSTR name=nullptr; hr=get_name(object,&name);
        if(SUCCEEDED(hr)) { if(result) { result->vt=VT_BSTR; result->bstrVal=name; } else SysFreeString(name); } return hr;
    }
    return DISP_E_MEMBERNOTFOUND;
}
// _Application is dual. Measured slots: Application=7, Parent=9, Run=42,
// Version=281 (includes LCID), Hwnd=351; 474 total slots on this Excel build.
// Window is a dispatch-only interface: its actual vtable has seven slots.
HRESULT WINAPI unsupported(Object*) { return E_NOTIMPL; }
HRESULT WINAPI early_run(Object* object,VARIANT macro,
    VARIANT a1,VARIANT a2,VARIANT a3,VARIANT a4,VARIANT a5,VARIANT a6,VARIANT a7,VARIANT a8,VARIANT a9,VARIANT a10,
    VARIANT a11,VARIANT a12,VARIANT a13,VARIANT a14,VARIANT a15,VARIANT a16,VARIANT a17,VARIANT a18,VARIANT a19,VARIANT a20,
    VARIANT a21,VARIANT a22,VARIANT a23,VARIANT a24,VARIANT a25,VARIANT a26,VARIANT a27,VARIANT a28,VARIANT a29,VARIANT a30,VARIANT* result) {
    VARIANT args[]={a30,a29,a28,a27,a26,a25,a24,a23,a22,a21,a20,a19,a18,a17,a16,a15,a14,a13,a12,a11,a10,a9,a8,a7,a6,a5,a4,a3,a2,a1,macro};
    DISPPARAMS params{args,nullptr,31,0};
    if(result) VariantInit(result);
    return run(object,&params,result);
}
std::array<void*,474> make_vtable() {
    std::array<void*,474> v;
    v.fill(reinterpret_cast<void*>(unsupported));
    v[0]=reinterpret_cast<void*>(query); v[1]=reinterpret_cast<void*>(add_ref); v[2]=reinterpret_cast<void*>(release);
    v[3]=reinterpret_cast<void*>(type_count); v[4]=reinterpret_cast<void*>(type_info);
    v[5]=reinterpret_cast<void*>(names); v[6]=reinterpret_cast<void*>(invoke);
    v[7]=reinterpret_cast<void*>(get_application); v[9]=reinterpret_cast<void*>(get_parent);
    v[223]=reinterpret_cast<void*>(get_path); v[300]=reinterpret_cast<void*>(get_enable_events); v[301]=reinterpret_cast<void*>(put_enable_events);
    v[52]=reinterpret_cast<void*>(get_workbooks);
    v[42]=reinterpret_cast<void*>(early_run); v[281]=reinterpret_cast<void*>(get_version); v[351]=reinterpret_cast<void*>(get_hwnd);
    return v;
}
auto application_vtable=make_vtable();
std::array<void*,7> window_vtable={application_vtable[0],application_vtable[1],application_vtable[2],application_vtable[3],application_vtable[4],application_vtable[5],application_vtable[6]};
std::array<void*,289> make_workbook_vtable() {
    std::array<void*,289> v; v.fill(reinterpret_cast<void*>(unsupported));
    for(size_t i=0;i<10;++i) v[i]=application_vtable[i];
    v[50]=reinterpret_cast<void*>(get_fullname); v[66]=reinterpret_cast<void*>(get_name); v[76]=reinterpret_cast<void*>(get_path); return v;
}
auto workbook_vtable=make_workbook_vtable();
std::array<void*,26> make_workbooks_vtable() {
    std::array<void*,26> v; v.fill(reinterpret_cast<void*>(unsupported));
    for(size_t i=0;i<10;++i) v[i]=application_vtable[i];
    v[12]=reinterpret_cast<void*>(get_count); v[13]=reinterpret_cast<void*>(get_item); return v;
}
auto workbooks_vtable=make_workbooks_vtable();
struct Projection { Object* app; Object* window; Object* workbook; Object* workbooks; };
}

extern "C" void* dna_com_create(size_t hwnd,const void* context,Run run) noexcept {
    try {
        auto state=std::make_shared<State>(State{reinterpret_cast<HWND>(hwnd),GetCurrentThreadId(),context,run});
        auto app=std::make_unique<Object>(Object{application_vtable.data(),1,state,nullptr,true});
        app->application=app.get();
        auto workbook=std::make_unique<Object>(Object{workbook_vtable.data(),1,state,app.get(),false,app.get(),true});
        auto workbooks=std::make_unique<Object>(Object{workbooks_vtable.data(),1,state,app.get(),false,app.get(),false,true});
        auto window=std::make_unique<Object>(Object{window_vtable.data(),1,state,app.get(),false,workbook.get()});
        auto projection=std::make_unique<Projection>(Projection{app.get(),window.get(),workbook.get(),workbooks.get()});
        state->workbook=workbook.get(); state->workbooks=workbooks.get();
        add_ref(app.get()); // Window's Application reference.
        add_ref(app.get()); // Workbook's Application reference.
        add_ref(workbook.get()); // Window's Parent reference.
        add_ref(app.get()); // Workbooks' Application reference.
        app.release(); workbook.release(); workbooks.release(); window.release(); return projection.release();
    } catch (...) { return nullptr; }
}
extern "C" LRESULT dna_com_getobject(void* pointer,WPARAM wparam,LPARAM object_id) noexcept {
    auto projection=static_cast<Projection*>(pointer);
    if(static_cast<LONG>(object_id)!=OBJID_NATIVEOM || FAILED(valid(projection->window))) return 0;
    return LresultFromObject(IID_IDispatch,wparam,reinterpret_cast<IUnknown*>(projection->window));
}
extern "C" void dna_com_destroy(void* pointer) noexcept {
    auto projection=static_cast<Projection*>(pointer);
    projection->app->state->connected=false;
    projection->app->state->context=nullptr;
    release(projection->window); release(projection->workbook); release(projection->workbooks); release(projection->app); delete projection;
}
extern "C" HRESULT dna_com_verify(void* pointer) noexcept {
    auto projection=static_cast<Projection*>(pointer);
    void* app=nullptr; HRESULT hr=query(projection->app,application_iid,&app); if(FAILED(hr)) return hr;
    auto object=static_cast<Object*>(app);
    auto version=reinterpret_cast<HRESULT(WINAPI*)(Object*,LONG,BSTR*)>(object->vtable[281]);
    BSTR text=nullptr; hr=version(object,0,&text);
    if(SUCCEEDED(hr) && (!text || wcscmp(text,L"16.0"))) hr=E_UNEXPECTED;
    SysFreeString(text);
    LONG hwnd=0;
    if(SUCCEEDED(hr)) hr=reinterpret_cast<HRESULT(WINAPI*)(Object*,LONG*)>(object->vtable[351])(object,&hwnd);
    if(SUCCEEDED(hr) && hwnd!=static_cast<LONG>(reinterpret_cast<INT_PTR>(object->state->hwnd))) hr=E_UNEXPECTED;
    release(object);
    void* window=nullptr;
    if(SUCCEEDED(hr)) hr=query(projection->window,window_iid,&window);
    if(window) release(static_cast<Object*>(window));
    if(SUCCEEDED(hr)) {
        DISPPARAMS params{}; VARIANT result{};
        if(invoke(object,0x7fffffff,IID_NULL,0,DISPATCH_PROPERTYGET,&params,&result,nullptr,nullptr)!=DISP_E_MEMBERNOTFOUND) hr=E_UNEXPECTED;
    }
    return hr;
}
extern "C" HRESULT dna_com_run_early(void* pointer,const wchar_t* name,size_t length,Value* output) noexcept {
    auto object=static_cast<Projection*>(pointer)->app;
    VARIANT macro{}; macro.vt=VT_BSTR; macro.bstrVal=SysAllocStringLen(name,static_cast<UINT>(length));
    if(!macro.bstrVal) return E_OUTOFMEMORY;
    VARIANT missing{}; missing.vt=VT_ERROR; missing.scode=DISP_E_PARAMNOTFOUND;
    VARIANT result{};
    auto entry=reinterpret_cast<decltype(&early_run)>(object->vtable[42]);
    HRESULT hr=entry(object,macro,
        missing,missing,missing,missing,missing,missing,missing,missing,missing,missing,
        missing,missing,missing,missing,missing,missing,missing,missing,missing,missing,
        missing,missing,missing,missing,missing,missing,missing,missing,missing,missing,&result);
    if(SUCCEEDED(hr)) {
        if(result.vt==VT_R8) { output->kind=1; output->number=result.dblVal; }
        else if(result.vt==VT_BOOL) { output->kind=3; output->number=result.boolVal!=VARIANT_FALSE; }
        else hr=E_NOTIMPL;
    }
    VariantClear(&macro); VariantClear(&result); return hr;
}
