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
#include <algorithm>
#include "value.h"

namespace {
constexpr GUID application_iid = {0x000208d5,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID window_iid = {0x00020893,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID workbook_iid = {0x000208da,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID workbooks_iid = {0x000208db,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID events_iid = {0x00024413,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID worksheet_iid = {0x000208d8,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID range_iid = {0x00020846,0,0,{0xc0,0,0,0,0,0,0,0x46}};
using Run = HRESULT (WINAPI*)(const void*,const wchar_t*,size_t,const Value*,size_t,Value*);
using Qualifier = HRESULT (WINAPI*)(const wchar_t*,size_t,const wchar_t*,size_t,BSTR*);
struct Area { unsigned row, column, end_row, end_column; };
struct RangeSpec { void* handle; unsigned long long sheet; BSTR name; const Area* areas; size_t count; };
using RangeApi = HRESULT (WINAPI*)(const void*,unsigned,void*,const wchar_t*,size_t,RangeSpec*);
using RangeRead = HRESULT (WINAPI*)(const void*,void*,bool,void*);
using RangeFree = void (WINAPI*)(void*);
using Bridge = HRESULT (WINAPI*)(const void*,unsigned,Value*);
using OwnsModule = bool (WINAPI*)(const void*,const wchar_t*,size_t);
struct Object;
struct ComAddinState;
enum class OfficeRole { None, Addins, Addin, Bars, Bar, Controls, Popup, Button };
struct State {
    HWND hwnd;
    DWORD thread;
    const void* context;
    Run run;
    Qualifier qualifier = nullptr;
    OwnsModule owns_module = nullptr;
    bool connected = true;
    bool enable_events = true;
    const void* workbook_context = nullptr;
    Bridge bridge = nullptr;
    RangeApi range_api = nullptr;
    RangeRead range_read = nullptr;
    RangeFree range_free = nullptr;
    Object* workbook = nullptr;
    Object* workbooks = nullptr;
    std::wstring workbook_name = L"Book1";
    Object* worksheet = nullptr;
    std::wstring worksheet_name;
    DWORD next_cookie = 1;
    std::vector<std::pair<DWORD,IUnknown*>> sinks;
    Object* addins = nullptr;
    Object* bars = nullptr;
    HRESULT office_failure = S_OK;
    bool office_closing = false;
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
    bool is_worksheet = false;
    bool is_range = false;
    unsigned row = 0, column = 0;
    OfficeRole office = OfficeRole::None;
    std::vector<Object*> children;
    Object* controls = nullptr;
    std::wstring caption, action;
    bool deleted = false; // ubs:ignore - Disconnection flag, not a memory deallocation.
    std::shared_ptr<ComAddinState> com_addin;
    std::vector<Area> areas;
    std::wstring sheet_name;
    unsigned long long sheet_id = 0;
    void* range_handle = nullptr;
    int range_view = 0;
};
bool retain_office_owner(Object*);
ULONG WINAPI add_ref(Object* object) { return InterlockedIncrement(&object->references); }
ULONG WINAPI release(Object* object) {
    LONG count = InterlockedDecrement(&object->references);
    if (!count) {
        if(retain_office_owner(object))return InterlockedIncrement(&object->references);
        if(object->range_handle && object->state->range_free) object->state->range_free(object->range_handle);
        for(auto child:object->children) release(child);
        if(object->controls) release(object->controls);
        if (!object->is_application) {
            if(object->parent && object->parent!=object->application) release(object->parent);
            release(object->application);
        }
        delete object; // ubs:ignore - COM Release frees the object only after its last reference.
    }
    return count;
}
HRESULT valid(Object* object) {
    if (!object->state->connected) return CO_E_OBJNOTCONNECTED;
    if(object->office!=OfficeRole::None&&object->deleted)return CO_E_OBJNOTCONNECTED; // ubs:ignore - Disconnection flag, not a memory deallocation.
    if (GetCurrentThreadId() != object->state->thread) return RPC_E_WRONG_THREAD;
    return S_OK;
}
HRESULT WINAPI query(Object*, REFIID, void**);
bool office_iid(Object*,REFIID);
HRESULT office_names(Object*,LPOLESTR*,UINT,DISPID*);
HRESULT office_invoke(Object*,DISPID,WORD,DISPPARAMS*,VARIANT*);
HRESULT WINAPI get_addins(Object*,IDispatch**);
HRESULT WINAPI get_bars(Object*,IDispatch**);
HRESULT WINAPI get_automation_security(Object*,LONG*);
HRESULT WINAPI put_automation_security(Object*,LONG);
HRESULT close_office(Object*);

// Subscriptions belong to the source, so a second FindConnectionPoint observes
// the same cookies. Event delivery snapshots references to allow reentrant Unadvise.
class Point final : public IConnectionPoint {
    volatile LONG references = 1;
    Object* owner;
public:
    explicit Point(Object* object) : owner(object) { add_ref(owner); }
    ~Point() { release(owner); }
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void** out) override {
        if (!out) return E_POINTER;
        *out=nullptr;
        if (iid!=IID_IUnknown && iid!=IID_IConnectionPoint) return E_NOINTERFACE;
        *out=static_cast<IConnectionPoint*>(this); AddRef(); return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&references); }
    ULONG STDMETHODCALLTYPE Release() override { LONG n=InterlockedDecrement(&references); if(!n) delete this; return n; } // ubs:ignore - COM Release frees the object only after its last reference.
    HRESULT STDMETHODCALLTYPE GetConnectionInterface(IID* iid) override { if(!iid) return E_POINTER; *iid=events_iid; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetConnectionPointContainer(IConnectionPointContainer** out) override { return query(owner,IID_IConnectionPointContainer,reinterpret_cast<void**>(out)); }
    HRESULT STDMETHODCALLTYPE Advise(IUnknown* sink,DWORD* cookie) override {
        if(!sink || !cookie) return E_POINTER;
        *cookie=0;
        HRESULT hr=valid(owner); if(FAILED(hr)) return hr;
        IUnknown* events=nullptr;
        hr=sink->QueryInterface(events_iid,reinterpret_cast<void**>(&events));
        if(FAILED(hr)) return CONNECT_E_CANNOTCONNECT;
        auto& state=*owner->state;
        if (!state.next_cookie) { events->Release(); return CONNECT_E_ADVISELIMIT; }
        DWORD id=state.next_cookie++;
        try { state.sinks.emplace_back(id,events); } catch (...) { events->Release(); return E_OUTOFMEMORY; }
        *cookie=id; return S_OK;
    }
    HRESULT STDMETHODCALLTYPE Unadvise(DWORD cookie) override {
        HRESULT hr=valid(owner); if(FAILED(hr)) return hr;
        auto& sinks=owner->state->sinks;
        for(auto it=sinks.begin();it!=sinks.end();++it) if(it->first==cookie) { auto sink=it->second; sinks.erase(it); sink->Release(); return S_OK; }
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
    ULONG STDMETHODCALLTYPE Release() override { LONG n=InterlockedDecrement(&references); if(!n) delete this; return n; } // ubs:ignore - COM Release frees the object only after its last reference.
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
    if(iid==IID_IUnknown || iid==IID_IDispatch || office_iid(object,iid) ||
       (object->office==OfficeRole::None && iid==(object->is_application?application_iid:(object->is_workbook?workbook_iid:(object->is_collection?workbooks_iid:(object->is_worksheet?worksheet_iid:(object->is_range?range_iid:window_iid))))))) {
        *out=object; add_ref(object); return S_OK;
    }
    if(object->is_application && iid==IID_IConnectionPointContainer) {
        *out=new(std::nothrow) Container(object); return *out?S_OK:E_OUTOFMEMORY;
    }
    return E_NOINTERFACE;
}
// The type information of a Range: a private dispinterface named "Range" built in memory with
// CreateTypeLib2 (nothing is saved or registered, and Excel's own type library is not read).
// VBA's TypeName of a late-bound object, which an add-in compiled from VBA asks, reads the name
// from IDispatch::GetTypeInfo(0), so "Range" is what `TypeName(Application.Range("A1"))` answers.
// The library object stays alive for the process: the type info refers to it.
ITypeInfo* range_type_info() {
    static ITypeInfo* info=nullptr;
    static bool tried=false;
    if(tried) return info;
    tried=true;
    ICreateTypeLib2* library=nullptr;
    if(FAILED(CreateTypeLib2(SYS_WIN64,L"DnaCalcObjects.tlb",&library))) return nullptr;
    ICreateTypeInfo* create=nullptr;
    if(SUCCEEDED(library->SetName(const_cast<LPOLESTR>(L"DnaCalcObjects"))) && SUCCEEDED(library->CreateTypeInfo(const_cast<LPOLESTR>(L"Range"),TKIND_DISPATCH,&create))) {
        if(SUCCEEDED(create->SetGuid(range_iid)) && SUCCEEDED(create->SetTypeFlags(TYPEFLAG_FDISPATCHABLE)) && SUCCEEDED(create->LayOut()))
            create->QueryInterface(IID_ITypeInfo,reinterpret_cast<void**>(&info));
        create->Release();
    }
    return info;
}
HRESULT WINAPI type_count(Object* object,UINT* count) { if(!count) return E_POINTER; *count=object->is_range&&range_type_info()?1:0; return S_OK; }
HRESULT WINAPI type_info(Object* object,UINT index,LCID,ITypeInfo** info) {
    if(!info) return E_POINTER;
    *info=nullptr;
    if(!object->is_range || index!=0 || !range_type_info()) return DISP_E_BADINDEX;
    *info=range_type_info(); (*info)->AddRef(); return S_OK;
}
HRESULT WINAPI names(Object* object,REFIID iid,LPOLESTR* names,UINT count,LCID,DISPID* ids) {
    if(iid!=IID_NULL) return DISP_E_UNKNOWNINTERFACE;
    if(!names || !ids) return E_POINTER;
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    if(object->office!=OfficeRole::None) return office_names(object,names,count,ids);
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
        else if(object->is_application && !_wcsicmp(names[i],L"COMAddIns")) ids[i]=0x704;
        else if(object->is_application && !_wcsicmp(names[i],L"CommandBars")) ids[i]=0x59f;
        else if(object->is_application && !_wcsicmp(names[i],L"AutomationSecurity")) ids[i]=0x795;
        else if((object->is_application || object->is_workbook) && !_wcsicmp(names[i],L"ActiveSheet")) ids[i]=0x133;
        else if(object->is_application && !_wcsicmp(names[i],L"ActiveWorkbook")) ids[i]=0x134;
        else if(object->is_application && !_wcsicmp(names[i],L"Range") && object->state->bridge) ids[i]=0xc5;
        else if(object->is_application && !_wcsicmp(names[i],L"Union") && object->state->bridge) ids[i]=0x30b;
        else if(object->is_application && !_wcsicmp(names[i],L"Caller")) ids[i]=0x13d;
        else if(object->is_application && !_wcsicmp(names[i],L"ThisCell")) ids[i]=0x7aa;
        else if((object->is_workbook || object->is_worksheet) && !_wcsicmp(names[i],L"Name")) ids[i]=0x6e;
        else if(object->is_workbook && !_wcsicmp(names[i],L"FullName")) ids[i]=0x121;
        else if(object->is_collection && !_wcsicmp(names[i],L"Count")) ids[i]=0x76;
        else if(object->is_collection && !_wcsicmp(names[i],L"Item")) ids[i]=0xaa;
        else if(object->is_range && !_wcsicmp(names[i],L"Value")) ids[i]=0x6;
        else if(object->is_range && !_wcsicmp(names[i],L"Value2")) ids[i]=0x56c;
        else if(object->is_range && !_wcsicmp(names[i],L"Address")) ids[i]=0xec;
        else if(object->is_range && !_wcsicmp(names[i],L"Row")) ids[i]=0x101;
        else if(object->is_range && !_wcsicmp(names[i],L"Column")) ids[i]=0xf0;
        else if(object->is_range && !_wcsicmp(names[i],L"Rows")) ids[i]=0x102;
        else if(object->is_range && !_wcsicmp(names[i],L"Columns")) ids[i]=0xf1;
        else if(object->is_range && !_wcsicmp(names[i],L"Count")) ids[i]=0x76;
        else if(object->is_range && !_wcsicmp(names[i],L"Worksheet")) ids[i]=0x15c;
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
HRESULT bridge_text(Object* object,unsigned operation,std::wstring& text) {
    auto& state=*object->state;
    if(!state.bridge) return E_NOTIMPL;
    Value value{};HRESULT hr=state.bridge(state.workbook_context,operation,&value);
    if(FAILED(hr)) return hr;
    try {text.assign(value.text,value.length);}catch(...){SysFreeString(const_cast<wchar_t*>(value.text));return E_OUTOFMEMORY;}
    SysFreeString(const_cast<wchar_t*>(value.text));return S_OK;
}
HRESULT book_identity(Object* object,std::wstring& name,std::wstring* full=nullptr,std::wstring* path=nullptr) {
    std::wstring text=object->state->workbook_name;
    if(object->state->bridge) {HRESULT hr=bridge_text(object,0,text);if(FAILED(hr))return hr;}
    auto slash=text.find_last_of(L"\\/");
    name=slash==std::wstring::npos?text:text.substr(slash+1);
    object->state->workbook_name=name;
    if(full)*full=text;
    if(path)*path=slash==std::wstring::npos?L"":text.substr(0,slash==0||(slash==2&&text[1]==L':')?slash+1:slash);
    return S_OK;
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
    else {std::wstring name;hr=book_identity(object,name,nullptr,&path);if(FAILED(hr))return hr;}
    *out=SysAllocStringLen(path.data(),static_cast<UINT>(path.size()));
    return *out?S_OK:E_OUTOFMEMORY;
}
HRESULT WINAPI get_enable_events(Object* object,VARIANT_BOOL* out) {
    if(!out) return E_POINTER;
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    bool enabled=object->state->enable_events;
    if(object->state->bridge){Value value{};hr=object->state->bridge(object->state->workbook_context,2,&value);if(FAILED(hr))return hr;enabled=value.number!=0.;}
    *out=enabled?VARIANT_TRUE:VARIANT_FALSE; return S_OK;
}
HRESULT WINAPI put_enable_events(Object* object,VARIANT_BOOL value) {
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    if(object->state->bridge){Value input{};input.number=value!=VARIANT_FALSE;return object->state->bridge(object->state->workbook_context,3,&input);}
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
    std::wstring name;hr=book_identity(object,name);if(FAILED(hr)){VariantClear(&value);return hr;}
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
    std::wstring name=object->sheet_name.empty()?object->state->worksheet_name:object->sheet_name;
    if(object->is_workbook){hr=book_identity(object,name);if(FAILED(hr))return hr;}
    *out=SysAllocString(name.c_str()); return *out?S_OK:E_OUTOFMEMORY;
}
HRESULT WINAPI get_active_sheet(Object* object,IDispatch** out) {
    if(!out) return E_POINTER;
    *out=nullptr; HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    if(object->state->bridge){std::wstring name;hr=bridge_text(object,1,name);if(FAILED(hr))return hr;if(name.empty())return E_NOTIMPL;object->state->worksheet_name=name;}
    auto sheet=object->state->worksheet;
    if(!sheet) return E_NOTIMPL;
    add_ref(sheet); *out=reinterpret_cast<IDispatch*>(sheet); return S_OK;
}
HRESULT WINAPI get_active_workbook(Object* object,IDispatch** out) {
    if(!out) return E_POINTER;
    *out=nullptr; HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    auto book=object->state->workbook;
    if(!book) return E_NOTIMPL;
    add_ref(book); *out=reinterpret_cast<IDispatch*>(book); return S_OK;
}
// Workbook handles belong to the session reference tracker. Metadata is refreshed on
// access, so structural edits and workbook replacement cannot silently reuse coordinates.
HRESULT range_metadata(Object* object) {
    if(!object->range_handle)return S_OK;
    auto& state=*object->state;RangeSpec spec{};
    HRESULT hr=state.range_api(state.workbook_context,2,object->range_handle,nullptr,0,&spec);if(FAILED(hr))return hr;
    try {object->areas.assign(spec.areas,spec.areas+spec.count);object->sheet_name.assign(spec.name,SysStringLen(spec.name));}
    catch(...){SysFreeString(spec.name);return E_OUTOFMEMORY;}
    SysFreeString(spec.name);object->sheet_id=spec.sheet;
    object->row=object->areas[0].row;object->column=object->areas[0].column;
    if(object->parent&&object->parent->is_worksheet)object->parent->sheet_name=object->sheet_name;
    return S_OK;
}
Object* make_range(Object* app,RangeSpec& spec,int view=0);
HRESULT give_range(Object* range,VARIANT* result) {
    if(!range)return E_OUTOFMEMORY;
    if(result){result->vt=VT_DISPATCH;result->pdispVal=reinterpret_cast<IDispatch*>(range);}else release(range);
    return S_OK;
}
Object* own_range(IDispatch* candidate,const std::shared_ptr<State>& state);
HRESULT spec_from_argument(Object* app,VARIANT& argument,RangeSpec& spec) {
    VARIANT value{};HRESULT hr=VariantCopyInd(&value,&argument);if(FAILED(hr))return hr;
    if(value.vt==VT_BSTR && value.bstrVal)hr=app->state->range_api(app->state->workbook_context,0,nullptr,value.bstrVal,SysStringLen(value.bstrVal),&spec);
    else if(value.vt==VT_DISPATCH) {
        auto range=own_range(value.pdispVal,app->state);
        if(!range)hr=DISP_E_TYPEMISMATCH;
        else {hr=range_metadata(range);if(SUCCEEDED(hr)){spec.sheet=range->sheet_id;spec.areas=range->areas.data();spec.count=range->areas.size();hr=app->state->range_api(app->state->workbook_context,3,nullptr,nullptr,0,&spec);}}
    } else hr=DISP_E_TYPEMISMATCH;
    VariantClear(&value);return hr;
}
HRESULT application_range(Object* app,DISPPARAMS* params,VARIANT* result) {
    if(!app->state->bridge||!app->state->range_api)return E_NOTIMPL;
    if(params->cNamedArgs)return DISP_E_NONAMEDARGS;
    if(params->cArgs<1||params->cArgs>2)return DISP_E_BADPARAMCOUNT;
    RangeSpec first{};HRESULT hr=spec_from_argument(app,params->rgvarg[params->cArgs-1],first);if(FAILED(hr))return hr;
    // Copy the first result before asking Rust for a second; its area slice is borrowed.
    auto one=make_range(app,first);if(!one)return E_OUTOFMEMORY;
    if(params->cArgs==2 && !(params->rgvarg[0].vt==VT_ERROR&&params->rgvarg[0].scode==DISP_E_PARAMNOTFOUND)) {
        RangeSpec second{};hr=spec_from_argument(app,params->rgvarg[0],second);
        if(FAILED(hr)){release(one);return hr;}
        auto two=make_range(app,second);if(!two){release(one);return E_OUTOFMEMORY;}
        if(one->sheet_id!=two->sheet_id||one->areas.size()!=1||two->areas.size()!=1){release(two);release(one);return static_cast<HRESULT>(0x800A03ECu);}
        auto a=one->areas[0],b=two->areas[0];Area area{(std::min)(a.row,b.row),(std::min)(a.column,b.column),(std::max)(a.end_row,b.end_row),(std::max)(a.end_column,b.end_column)};
        RangeSpec joined{};joined.sheet=one->sheet_id;joined.areas=&area;joined.count=1;
        hr=app->state->range_api(app->state->workbook_context,3,nullptr,nullptr,0,&joined);
        release(two);release(one);if(FAILED(hr))return hr;
        one=make_range(app,joined);
    }
    return give_range(one,result);
}
HRESULT application_union(Object* app,DISPPARAMS* params,VARIANT* result) {
    try {
    if(!app->state->bridge||!app->state->range_api)return E_NOTIMPL;
    if(params->cNamedArgs)return DISP_E_NONAMEDARGS;
    if(params->cArgs<2||params->cArgs>30)return DISP_E_BADPARAMCOUNT;
    std::vector<Area> areas;unsigned long long sheet=0;bool have_sheet=false;
    for(UINT i=params->cArgs;i-->0;) {
        auto& argument=params->rgvarg[i];if(argument.vt==VT_ERROR&&argument.scode==DISP_E_PARAMNOTFOUND)continue;
        VARIANT value{};HRESULT hr=VariantCopyInd(&value,&argument);if(FAILED(hr))return hr;
        auto range=value.vt==VT_DISPATCH?own_range(value.pdispVal,app->state):nullptr;
        if(!range){VariantClear(&value);return DISP_E_TYPEMISMATCH;}
        hr=range_metadata(range);if(FAILED(hr)){VariantClear(&value);return hr;}
        if(have_sheet&&sheet!=range->sheet_id){VariantClear(&value);return static_cast<HRESULT>(0x800A03ECu);}
        sheet=range->sheet_id;have_sheet=true;areas.insert(areas.end(),range->areas.begin(),range->areas.end());VariantClear(&value);
    }
    if(!have_sheet)return DISP_E_BADPARAMCOUNT;
    RangeSpec spec{};spec.sheet=sheet;spec.areas=areas.data();spec.count=areas.size();
    HRESULT hr=app->state->range_api(app->state->workbook_context,3,nullptr,nullptr,0,&spec);if(FAILED(hr))return hr;
    return give_range(make_range(app,spec),result);
    }catch(const std::bad_alloc&){return E_OUTOFMEMORY;}catch(...){return E_FAIL;}
}
HRESULT application_caller(Object* app,DISPPARAMS* params,VARIANT* result) {
    if(!app->state->bridge||!app->state->range_api)return E_NOTIMPL;
    if(params->cNamedArgs)return DISP_E_NONAMEDARGS;
    if(params->cArgs>1)return DISP_E_BADPARAMCOUNT;
    if(params->cArgs && !(params->rgvarg[0].vt==VT_ERROR&&params->rgvarg[0].scode==DISP_E_PARAMNOTFOUND))return E_NOTIMPL;
    RangeSpec spec{};HRESULT hr=app->state->range_api(app->state->workbook_context,1,nullptr,nullptr,0,&spec);
    if(hr==S_FALSE){if(result){result->vt=VT_ERROR;result->scode=static_cast<SCODE>(0x800A07E7u);}return S_OK;}
    if(FAILED(hr))return hr;
    return give_range(make_range(app,spec),result);
}
HRESULT range_value(Object* object,bool value2,VARIANT* result) {
    if(!result)return E_POINTER;
    if(!object->state->bridge||!object->range_handle)return E_NOTIMPL;
    return object->state->range_read(object->state->workbook_context,object->range_handle,value2,result);
}
HRESULT range_address(Object* object,DISPPARAMS* params,VARIANT* result) {
    HRESULT metadata=range_metadata(object);if(FAILED(metadata))return metadata;
    if(params->cNamedArgs) return DISP_E_NONAMEDARGS;
    if(params->cArgs>5) return DISP_E_BADPARAMCOUNT;
    bool absolute_row=true,absolute_column=true,external=false;
    LONG style=1;
    for(UINT i=0;i<params->cArgs;++i) {
        auto& arg=params->rgvarg[params->cArgs-1-i];
        if(arg.vt==VT_ERROR && arg.scode==DISP_E_PARAMNOTFOUND) continue;
        if(i==4) return E_NOTIMPL; // RelativeTo requires a general range projection.
        VARIANT value{};
        HRESULT hr=VariantChangeType(&value,&arg,0,i==2?VT_I4:VT_BOOL);
        if(FAILED(hr)) return hr;
        if(i==0) absolute_row=value.boolVal!=VARIANT_FALSE;
        else if(i==1) absolute_column=value.boolVal!=VARIANT_FALSE;
        else if(i==2) style=value.lVal;
        else external=value.boolVal!=VARIANT_FALSE;
        VariantClear(&value);
    }
    std::wstring text;
    if(external) {
        auto& state=*object->state;
        if(!state.qualifier) return E_NOTIMPL;
        BSTR prefix=nullptr;
        std::wstring name;HRESULT hr=book_identity(object,name);if(FAILED(hr))return hr;
        const auto& sheet=object->sheet_name.empty()?state.worksheet_name:object->sheet_name;
        hr=state.qualifier(name.data(),name.size(),sheet.data(),sheet.size(),&prefix);
        if(FAILED(hr)) return hr;
        text.assign(prefix,SysStringLen(prefix)); SysFreeString(prefix); text+=L"!";
    }
    auto cell=[&](unsigned row,unsigned col) {
        if(style==-4150 && absolute_row && absolute_column) return L"R"+std::to_wstring(row+1)+L"C"+std::to_wstring(col+1);
        std::wstring column;unsigned n=col+1;while(n){column.insert(column.begin(),static_cast<wchar_t>(L'A'+(n-1)%26));n=(n-1)/26;}
        return std::wstring(absolute_column?L"$":L"")+column+(absolute_row?L"$":L"")+std::to_wstring(row+1);
    };
    if(style!=1 && !(style==-4150 && absolute_row && absolute_column))return E_NOTIMPL;
    auto areas=object->areas;if(areas.empty())areas.push_back({object->row,object->column,object->row,object->column});
    for(size_t i=0;i<areas.size();++i){if(i)text+=L",";auto a=areas[i];text+=cell(a.row,a.column);if(a.row!=a.end_row||a.column!=a.end_column)text+=L":"+cell(a.end_row,a.end_column);}
    if(result) { result->vt=VT_BSTR; result->bstrVal=SysAllocString(text.c_str()); if(!result->bstrVal) return E_OUTOFMEMORY; }
    return S_OK;
}
HRESULT WINAPI get_fullname(Object* object,LONG,BSTR* out) {
    if(!out)return E_POINTER;*out=nullptr;HRESULT hr=valid(object);if(FAILED(hr))return hr;
    std::wstring name,full;hr=book_identity(object,name,&full);if(FAILED(hr))return hr;
    *out=SysAllocString(full.c_str());return *out?S_OK:E_OUTOFMEMORY;
}
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
            else if(output.kind==4) { result->vt=VT_I4; result->lVal=static_cast<LONG>(output.number); }
            else if(output.kind==3) { result->vt=VT_BOOL; result->boolVal=output.number?VARIANT_TRUE:VARIANT_FALSE; }
            else hr=E_NOTIMPL;
        }
    }
    for(auto& v:indirect) VariantClear(&v);
    VariantClear(&macro);
    return hr;
}
// Values-only profile (dc-svqh): what needs a workbook is refused with Excel's 1004 and this text.
HRESULT needs_workbook(EXCEPINFO* error) {
    if(!error) return DISP_E_MEMBERNOTFOUND;
    *error=EXCEPINFO{};
    error->wCode=1004;
    error->scode=static_cast<SCODE>(0x800A03ECu);
    error->bstrSource=SysAllocString(L"DNA OneCalc");
    error->bstrDescription=SysAllocString(L"needs a workbook: open in DNA Calc");
    return DISP_E_EXCEPTION;
}
HRESULT workbook_dispatch_result(HRESULT status,EXCEPINFO* error) {
    if(status!=static_cast<HRESULT>(0x800A03ECu))return status;
    if(error){
        *error=EXCEPINFO{};error->wCode=1004;error->scode=status;
        error->bstrSource=SysAllocString(L"DNA Calc");
        error->bstrDescription=SysAllocString(L"Workbook range operation failed");
    }
    return DISP_E_EXCEPTION;
}
HRESULT invoke_impl(Object* object,DISPID id,REFIID iid,LCID,WORD flags,DISPPARAMS* params,VARIANT* result,EXCEPINFO* error,UINT*) {
    if(result) VariantInit(result);
    if(iid!=IID_NULL) return DISP_E_UNKNOWNINTERFACE;
    if(!params || (params->cArgs && !params->rgvarg)) return E_POINTER;
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    if(object->office!=OfficeRole::None) {
        try{return office_invoke(object,id,flags,params,result);}catch(const std::bad_alloc&){return E_OUTOFMEMORY;}catch(...){return E_FAIL;}
    }
    if(object->is_application && (id==0x704 || id==0x59f) && (flags&DISPATCH_PROPERTYGET)) {
        if(params->cArgs) return DISP_E_BADPARAMCOUNT;
        IDispatch* value=nullptr;hr=id==0x704?get_addins(object,&value):get_bars(object,&value);
        if(SUCCEEDED(hr)) {if(result){result->vt=VT_DISPATCH;result->pdispVal=value;}else value->Release();} return hr;
    }
    if(object->is_application && id==0x795) {
        if(flags&DISPATCH_PROPERTYGET) {if(params->cArgs)return DISP_E_BADPARAMCOUNT;LONG value;hr=get_automation_security(object,&value);if(SUCCEEDED(hr)&&result){result->vt=VT_I4;result->lVal=value;}return hr;}
        return E_NOTIMPL;
    }
    if(object->is_application && id==0x103 && (flags&DISPATCH_METHOD)) return run(object,params,result);
    if(object->is_application && (id==0x13d||id==0x7aa) && (flags&DISPATCH_PROPERTYGET)) {
        if(!object->state->bridge)return needs_workbook(error);
        if(id==0x7aa&&params->cArgs)return DISP_E_BADPARAMCOUNT;
        return workbook_dispatch_result(application_caller(object,params,result),error);
    }
    if(object->is_application&&id==0xc5&&(flags&(DISPATCH_PROPERTYGET|DISPATCH_METHOD)))return workbook_dispatch_result(application_range(object,params,result),error);
    if(object->is_application&&id==0x30b&&(flags&DISPATCH_METHOD))return workbook_dispatch_result(application_union(object,params,result),error);
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
    if(object->is_range && id==0xec) return workbook_dispatch_result(range_address(object,params,result),error);
    if(object->is_range&&(id==0x6||id==0x56c||id==DISPID_VALUE)) {
        if(params->cArgs && !(id==0x6&&params->cArgs==1&&params->rgvarg[0].vt==VT_ERROR&&params->rgvarg[0].scode==DISP_E_PARAMNOTFOUND))return DISP_E_BADPARAMCOUNT;
        return workbook_dispatch_result(range_value(object,id==0x56c,result),error);
    }
    if(object->is_range){hr=range_metadata(object);if(FAILED(hr))return workbook_dispatch_result(hr,error);}
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
    if((id==0x133 && (object->is_application || object->is_workbook)) || (id==0x134 && object->is_application) || (id==0x15c && object->is_range)) {
        IDispatch* value=nullptr; hr=id==0x134?get_active_workbook(object,&value):(object->is_range?get_parent(object,&value):get_active_sheet(object,&value));
        if(SUCCEEDED(hr)) { if(result) { result->vt=VT_DISPATCH; result->pdispVal=value; } else value->Release(); } return hr;
    }
    if(object->is_range && (id==0x101 || id==0xf0 || id==0x76)) {
        if(result) {double count=0;for(auto a:object->areas)count+=double(a.end_row-a.row+1)*(a.end_column-a.column+1);if(object->areas.empty())count=1;
            if(!object->areas.empty()&&object->range_view==1)count=object->areas[0].end_row-object->areas[0].row+1;
            if(!object->areas.empty()&&object->range_view==2)count=object->areas[0].end_column-object->areas[0].column+1;
            double value=id==0x101?object->row+1:(id==0xf0?object->column+1:count);
            if(value<=LONG_MAX){result->vt=VT_I4;result->lVal=static_cast<LONG>(value);}else{result->vt=VT_R8;result->dblVal=value;}}
        return S_OK;
    }
    if(object->is_range && (id==0x102 || id==0xf1)) {
        if(object->range_handle){RangeSpec spec{};spec.sheet=object->sheet_id;spec.areas=object->areas.data();spec.count=object->areas.size();
            hr=object->state->range_api(object->state->workbook_context,3,nullptr,nullptr,0,&spec);if(FAILED(hr))return hr;
            return give_range(make_range(object->application,spec,id==0x102?1:2),result);}
        if(result) { result->vt=VT_DISPATCH; add_ref(object); result->pdispVal=reinterpret_cast<IDispatch*>(object); } return S_OK;
    }
    if(id==0x76 && object->is_collection) { LONG count; hr=get_count(object,&count); if(SUCCEEDED(hr) && result) { result->vt=VT_I4; result->lVal=count; } return hr; }
    if((id==0x6e && (object->is_workbook || object->is_worksheet)) || (id==0x121 && object->is_workbook)) {
        BSTR name=nullptr; hr=id==0x121?get_fullname(object,0,&name):get_name(object,&name);
        if(SUCCEEDED(hr)) { if(result) { result->vt=VT_BSTR; result->bstrVal=name; } else SysFreeString(name); } return hr;
    }
    return DISP_E_MEMBERNOTFOUND;
}
// No C++ allocation exception crosses the COM ABI.
HRESULT WINAPI invoke(Object* object,DISPID id,REFIID iid,LCID lcid,WORD flags,DISPPARAMS* params,VARIANT* result,EXCEPINFO* error,UINT* argument) {
    try {return invoke_impl(object,id,iid,lcid,flags,params,result,error,argument);}
    catch(const std::bad_alloc&){return E_OUTOFMEMORY;}
    catch(...){return E_FAIL;}
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
HRESULT WINAPI early_range(Object* object,VARIANT cell1,VARIANT cell2,IDispatch** out) {
    if(!out) return E_POINTER;
    *out=nullptr;
    VARIANT args[]={cell2,cell1};
    DISPPARAMS params{args,nullptr,2,0};
    VARIANT result{};
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    hr=application_range(object,&params,&result);
    if(SUCCEEDED(hr) && result.vt==VT_DISPATCH) *out=result.pdispVal;
    return hr;
}
// Caller and Union carry an [lcid] parameter before the return value (Excel's type library).
HRESULT WINAPI early_caller(Object* object,VARIANT index,LONG,VARIANT* out) {
    if(!out) return E_POINTER;
    VariantInit(out);
    VARIANT args[]={index};
    DISPPARAMS params{args,nullptr,1,0};
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    return application_caller(object,&params,out);
}
HRESULT WINAPI early_union(Object* object,IDispatch* a1,IDispatch* a2,
    VARIANT a3,VARIANT a4,VARIANT a5,VARIANT a6,VARIANT a7,VARIANT a8,VARIANT a9,VARIANT a10,
    VARIANT a11,VARIANT a12,VARIANT a13,VARIANT a14,VARIANT a15,VARIANT a16,VARIANT a17,VARIANT a18,VARIANT a19,VARIANT a20,
    VARIANT a21,VARIANT a22,VARIANT a23,VARIANT a24,VARIANT a25,VARIANT a26,VARIANT a27,VARIANT a28,VARIANT a29,VARIANT a30,LONG,IDispatch** out) {
    if(!out) return E_POINTER;
    *out=nullptr;
    VARIANT first{},second{};
    first.vt=VT_DISPATCH; first.pdispVal=a1; second.vt=VT_DISPATCH; second.pdispVal=a2;
    VARIANT args[]={a30,a29,a28,a27,a26,a25,a24,a23,a22,a21,a20,a19,a18,a17,a16,a15,a14,a13,a12,a11,a10,a9,a8,a7,a6,a5,a4,a3,second,first};
    DISPPARAMS params{args,nullptr,30,0};
    VARIANT result{};
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    hr=application_union(object,&params,&result);
    if(SUCCEEDED(hr) && result.vt==VT_DISPATCH) *out=result.pdispVal;
    return hr;
}
std::array<void*,474> make_vtable() {
    std::array<void*,474> v;
    v.fill(reinterpret_cast<void*>(unsupported));
    v[0]=reinterpret_cast<void*>(query); v[1]=reinterpret_cast<void*>(add_ref); v[2]=reinterpret_cast<void*>(release);
    v[3]=reinterpret_cast<void*>(type_count); v[4]=reinterpret_cast<void*>(type_info);
    v[5]=reinterpret_cast<void*>(names); v[6]=reinterpret_cast<void*>(invoke);
    v[7]=reinterpret_cast<void*>(get_application); v[9]=reinterpret_cast<void*>(get_parent);
    v[223]=reinterpret_cast<void*>(get_path); v[300]=reinterpret_cast<void*>(get_enable_events); v[301]=reinterpret_cast<void*>(put_enable_events);
    v[40]=reinterpret_cast<void*>(early_range);v[50]=reinterpret_cast<void*>(early_union);v[74]=reinterpret_cast<void*>(early_caller);
    v[52]=reinterpret_cast<void*>(get_workbooks);
    v[16]=reinterpret_cast<void*>(get_active_sheet); v[18]=reinterpret_cast<void*>(get_active_workbook);
    v[42]=reinterpret_cast<void*>(early_run); v[281]=reinterpret_cast<void*>(get_version); v[351]=reinterpret_cast<void*>(get_hwnd);
    v[25]=reinterpret_cast<void*>(get_bars);v[309]=reinterpret_cast<void*>(get_addins);
    v[339]=reinterpret_cast<void*>(get_automation_security);v[340]=reinterpret_cast<void*>(put_automation_security);
    return v;
}
auto application_vtable=make_vtable();
std::array<void*,7> window_vtable={application_vtable[0],application_vtable[1],application_vtable[2],application_vtable[3],application_vtable[4],application_vtable[5],application_vtable[6]};
std::array<void*,289> make_workbook_vtable() {
    std::array<void*,289> v; v.fill(reinterpret_cast<void*>(unsupported));
    for(size_t i=0;i<10;++i) v[i]=application_vtable[i];
    v[14]=reinterpret_cast<void*>(get_active_sheet); v[50]=reinterpret_cast<void*>(get_fullname); v[66]=reinterpret_cast<void*>(get_name); v[76]=reinterpret_cast<void*>(get_path); return v;
}
auto workbook_vtable=make_workbook_vtable();
std::array<void*,26> make_workbooks_vtable() {
    std::array<void*,26> v; v.fill(reinterpret_cast<void*>(unsupported));
    for(size_t i=0;i<10;++i) v[i]=application_vtable[i];
    v[12]=reinterpret_cast<void*>(get_count); v[13]=reinterpret_cast<void*>(get_item); return v;
}
auto workbooks_vtable=make_workbooks_vtable();
std::array<void*,158> make_worksheet_vtable() {
    std::array<void*,158> v; v.fill(reinterpret_cast<void*>(unsupported));
    for(size_t i=0;i<10;++i) v[i]=application_vtable[i];
    v[18]=reinterpret_cast<void*>(get_name); return v;
}
auto worksheet_vtable=make_worksheet_vtable();
Object* make_range(Object* app,RangeSpec& spec,int view) {
    Object* sheet=nullptr;Object* range=nullptr;
    try {
        sheet=new Object{worksheet_vtable.data(),1,app->state,app,false,app->state->workbook,false,false,true};
        add_ref(app);add_ref(app->state->workbook);
        sheet->sheet_name.assign(spec.name,SysStringLen(spec.name));sheet->sheet_id=spec.sheet;
        range=new Object{window_vtable.data(),1,app->state,app,false,sheet,false,false,false,true};
        add_ref(app);add_ref(sheet);
        range->areas.assign(spec.areas,spec.areas+spec.count);range->sheet_name=sheet->sheet_name;range->sheet_id=spec.sheet;
        range->row=range->areas[0].row;range->column=range->areas[0].column;range->range_view=view;
        range->range_handle=spec.handle;spec.handle=nullptr;
        SysFreeString(spec.name);spec.name=nullptr;release(sheet);return range;
    }catch(...){if(range)release(range);if(sheet)release(sheet);if(spec.handle)app->state->range_free(spec.handle);SysFreeString(spec.name);return nullptr;}
}
Object* own_range(IDispatch* candidate,const std::shared_ptr<State>& state) {
    if(!candidate||*reinterpret_cast<void***>(candidate)!=window_vtable.data())return nullptr;
    auto object=reinterpret_cast<Object*>(candidate);
    return object->is_range&&object->range_handle&&object->state==state?object:nullptr;
}
#include "projection-office.h"
struct Projection { Object* app; Object* window; Object* workbook; Object* workbooks; Object* worksheet; };

HRESULT fire(Object* object,DISPID id,VARIANT* args,UINT count) {
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    VARIANT_BOOL enabled;hr=get_enable_events(object,&enabled);if(FAILED(hr))return hr;if(!enabled)return S_OK;
    std::vector<IUnknown*> snapshot;
    try { for(auto& item:object->state->sinks) { snapshot.push_back(item.second); item.second->AddRef(); } }
    catch(...) { for(auto sink:snapshot) sink->Release(); return E_OUTOFMEMORY; }
    HRESULT first=S_OK;
    for(auto sink:snapshot) {
        IDispatch* dispatch=nullptr;
        hr=sink->QueryInterface(IID_IDispatch,reinterpret_cast<void**>(&dispatch));
        if(SUCCEEDED(hr)) {
            // Excel's AppEvents use declaration order plus named DISPIDs [0,1],
            // measured in X0. This differs from an ordinary positional Invoke.
            DISPID named[]={0,1}; DISPPARAMS params{args,count?named:nullptr,count,count};
            EXCEPINFO error{}; UINT arg_error=0;
            hr=dispatch->Invoke(id,IID_NULL,0,DISPATCH_METHOD|DISPATCH_PROPERTYGET,&params,nullptr,&error,&arg_error);
            SysFreeString(error.bstrSource); SysFreeString(error.bstrDescription); SysFreeString(error.bstrHelpFile);
            dispatch->Release();
        }
        if(FAILED(hr) && SUCCEEDED(first)) first=hr;
        sink->Release();
    }
    return first;
}
}

extern "C" void* dna_com_create(size_t hwnd,const void* context,Run run,Qualifier qualifier,OwnsModule owns_module) noexcept {
    try {
        auto state=std::make_shared<State>(State{reinterpret_cast<HWND>(hwnd),GetCurrentThreadId(),context,run});
        state->qualifier=qualifier;
        state->owns_module=owns_module;
        auto app=std::make_unique<Object>(Object{application_vtable.data(),1,state,nullptr,true});
        app->application=app.get();
        auto workbook=std::make_unique<Object>(Object{workbook_vtable.data(),1,state,app.get(),false,app.get(),true});
        auto workbooks=std::make_unique<Object>(Object{workbooks_vtable.data(),1,state,app.get(),false,app.get(),false,true});
        auto window=std::make_unique<Object>(Object{window_vtable.data(),1,state,app.get(),false,workbook.get()});
        auto worksheet=std::make_unique<Object>(Object{worksheet_vtable.data(),1,state,app.get(),false,workbook.get(),false,false,true});
        auto projection=std::make_unique<Projection>(Projection{app.get(),window.get(),workbook.get(),workbooks.get(),worksheet.get()});
        state->workbook=workbook.get(); state->workbooks=workbooks.get();
        state->worksheet=worksheet.get();
        add_ref(app.get()); // Window's Application reference.
        add_ref(app.get()); // Workbook's Application reference.
        add_ref(workbook.get()); // Window's Parent reference.
        add_ref(app.get()); // Workbooks' Application reference.
        add_ref(app.get()); add_ref(workbook.get()); // Worksheet's Application and Parent.
        app.release(); workbook.release(); workbooks.release(); window.release(); worksheet.release(); return projection.release();
    } catch (...) { return nullptr; }
}
extern "C" LRESULT dna_com_getobject(void* pointer,WPARAM wparam,LPARAM object_id) noexcept {
    auto projection=static_cast<Projection*>(pointer);
    if(static_cast<LONG>(object_id)!=OBJID_NATIVEOM || FAILED(valid(projection->window))) return 0;
    return LresultFromObject(IID_IDispatch,wparam,reinterpret_cast<IUnknown*>(projection->window));
}
extern "C" void dna_com_destroy(void* pointer) noexcept {
    auto projection=static_cast<Projection*>(pointer);
    try{close_office(projection->app);}catch(...){projection->app->state->office_failure=E_OUTOFMEMORY;projection->app->state->office_closing=true;}
    projection->app->state->connected=false;
    projection->app->state->context=nullptr;
    auto sinks=std::move(projection->app->state->sinks);
    for(auto& sink:sinks) sink.second->Release();
    if(projection->app->state->addins)release(projection->app->state->addins);
    if(projection->app->state->bars)release(projection->app->state->bars);
    release(projection->window); release(projection->worksheet); release(projection->workbook); release(projection->workbooks); release(projection->app); delete projection; // ubs:ignore - Projection exclusively owns the window objects released above.
}
extern "C" HRESULT dna_com_close_office(void* pointer) noexcept {
    try{return close_office(static_cast<Projection*>(pointer)->app);}catch(...){return E_OUTOFMEMORY;}
}
using RibbonVisitor=void (WINAPI*)(void*,const wchar_t*,size_t,const wchar_t*,size_t);
extern "C" HRESULT dna_com_ribbons(void* pointer,RibbonVisitor visit,void* context) noexcept {
    try {
        auto app=static_cast<Projection*>(pointer)->app;HRESULT hr=valid(app);if(FAILED(hr))return hr;
        hr=read_ribbon_ui(app);if(FAILED(hr))return hr;
        if(app->state->addins)for(auto item:app->state->addins->children){auto& d=*item->com_addin;if(d.connected&&d.ui_requested&&!d.xml.empty())visit(context,d.progid.data(),d.progid.size(),d.xml.data(),d.xml.size());}
        return S_OK;
    }catch(...){return E_OUTOFMEMORY;}
}
extern "C" HRESULT dna_com_ranges(void* pointer,RangeApi api,RangeRead read,RangeFree free) noexcept {
    auto app=static_cast<Projection*>(pointer)->app;HRESULT hr=valid(app);if(FAILED(hr))return hr;
    app->state->range_api=api;app->state->range_read=read;app->state->range_free=free;return S_OK;
}
HRESULT scalar_value(const Value& value,VARIANT* output) {
    VariantInit(output);
    switch(value.kind){
    case 0:return S_OK;
    case 1:output->vt=VT_R8;output->dblVal=value.number;return S_OK;
    case 2:output->vt=VT_BSTR;output->bstrVal=SysAllocStringLen(value.text,static_cast<UINT>(value.length));return output->bstrVal?S_OK:E_OUTOFMEMORY;
    case 3:output->vt=VT_BOOL;output->boolVal=value.number?VARIANT_TRUE:VARIANT_FALSE;return S_OK;
    case 5:output->vt=VT_ERROR;output->scode=static_cast<SCODE>(0x800A0000u|static_cast<unsigned>(value.number));return S_OK;
    default:return DISP_E_TYPEMISMATCH;
    }
}
extern "C" HRESULT dna_com_values(void* result,const Value* values,unsigned rows,unsigned columns) noexcept {
    if(!result||!values)return E_POINTER;auto output=static_cast<VARIANT*>(result);VariantInit(output);
    if(!rows&&!columns)return scalar_value(*values,output);
    if(!rows||!columns)return E_INVALIDARG;
    SAFEARRAYBOUND bounds[]={{rows,1},{columns,1}};auto array=SafeArrayCreate(VT_VARIANT,2,bounds);if(!array)return E_OUTOFMEMORY;
    HRESULT hr=S_OK;
    for(unsigned row=0;row<rows&&SUCCEEDED(hr);++row)for(unsigned column=0;column<columns&&SUCCEEDED(hr);++column){
        VARIANT value{};hr=scalar_value(values[static_cast<size_t>(row)*columns+column],&value);
        LONG indices[]={static_cast<LONG>(row+1),static_cast<LONG>(column+1)};
        if(SUCCEEDED(hr))hr=SafeArrayPutElement(array,indices,&value);VariantClear(&value);
    }
    if(FAILED(hr)){SafeArrayDestroy(array);return hr;}
    output->vt=VT_ARRAY|VT_VARIANT;output->parray=array;return S_OK;
}
extern "C" HRESULT dna_com_bridge(void* pointer,const void* context,Bridge bridge) noexcept {
    auto app=static_cast<Projection*>(pointer)->app;HRESULT hr=valid(app);if(FAILED(hr))return hr;
    app->state->workbook_context=context;app->state->bridge=bridge;return S_OK;
}
extern "C" HRESULT dna_com_workbook(void* pointer,const wchar_t* name,size_t length) noexcept {
    try {
        auto app=static_cast<Projection*>(pointer)->app; HRESULT hr=valid(app); if(FAILED(hr)) return hr;
        if(!name || !length) return E_INVALIDARG;
        app->state->workbook_name.assign(name,length); return S_OK;
    }catch(...) {return E_OUTOFMEMORY;}
}
extern "C" HRESULT dna_com_sheet(void* pointer,const wchar_t* name,size_t length) noexcept {
    try { auto app=static_cast<Projection*>(pointer)->app; HRESULT hr=valid(app); if(FAILED(hr)) return hr; app->state->worksheet_name.assign(name,length); return S_OK; }
    catch(...) { return E_OUTOFMEMORY; }
}
extern "C" HRESULT dna_com_enable_events(void* pointer,bool enabled) noexcept {
    return put_enable_events(static_cast<Projection*>(pointer)->app,enabled?VARIANT_TRUE:VARIANT_FALSE);
}
extern "C" HRESULT dna_com_sheet_change_areas(void* pointer,unsigned long long sheet_id,const wchar_t* name,size_t length,const Area* areas,size_t count) noexcept {
    try {
        auto projection=static_cast<Projection*>(pointer);auto app=projection->app;HRESULT hr=valid(app);if(FAILED(hr))return hr;
        if(!areas||!count)return E_INVALIDARG;
        for(size_t i=0;i<count;++i)if(areas[i].row>areas[i].end_row||areas[i].column>areas[i].end_column||areas[i].end_row>=1048576||areas[i].end_column>=16384)return E_INVALIDARG;
        Object* range=nullptr;
        if(app->state->bridge&&app->state->range_api) {
            RangeSpec spec{};spec.sheet=sheet_id;spec.areas=areas;spec.count=count;
            hr=app->state->range_api(app->state->workbook_context,3,nullptr,nullptr,0,&spec);if(FAILED(hr))return hr;
            range=make_range(app,spec);if(!range)return E_OUTOFMEMORY;
        } else {
            auto sheet=new Object{worksheet_vtable.data(),1,app->state,app,false,projection->workbook,false,false,true};
            add_ref(app);add_ref(projection->workbook);
            range=new(std::nothrow) Object{window_vtable.data(),1,app->state,app,false,sheet,false,false,false,true,areas[0].row,areas[0].column};
            if(!range){release(sheet);return E_OUTOFMEMORY;}
            add_ref(app);add_ref(sheet);
            try {sheet->sheet_name.assign(name,length);range->sheet_name=sheet->sheet_name;range->areas.assign(areas,areas+count);}
            catch(...){release(range);release(sheet);return E_OUTOFMEMORY;}
            release(sheet);
        }
        VARIANT args[2]{};args[0].vt=VT_DISPATCH;args[0].pdispVal=reinterpret_cast<IDispatch*>(range->parent);
        args[1].vt=VT_DISPATCH;args[1].pdispVal=reinterpret_cast<IDispatch*>(range);
        hr=fire(app,0x61c,args,2);release(range);return hr;
    }catch(...){return E_OUTOFMEMORY;}
}
extern "C" HRESULT dna_com_sheet_change(void* pointer,unsigned row,unsigned column) noexcept {
    auto& name=static_cast<Projection*>(pointer)->app->state->worksheet_name;Area area{row,column,row,column};
    return dna_com_sheet_change_areas(pointer,0,name.data(),name.size(),&area,1);
}
extern "C" HRESULT dna_com_after_calculate(void* pointer) noexcept {
    return fire(static_cast<Projection*>(pointer)->app,0xa34,nullptr,0);
}
// Exercises source lifetime and reentrant subscription removal independently of
// xlOil. The application route is separately measured with the original plugin.
namespace {
class EventProbe final : public IDispatch {
    LONG references=1;
public:
    unsigned changes=0,calculations=0;
    std::wstring expected_address=L"$D$3";
    LONG expected_count=1;
    HRESULT failure=S_OK;
    IConnectionPoint* remove_from=nullptr;
    DWORD cookie=0;
    IDispatch* retained=nullptr;
    ~EventProbe() { if(retained) retained->Release(); }
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void** out) override {
        if(!out) return E_POINTER;
        *out=nullptr;
        if(iid!=IID_IUnknown && iid!=IID_IDispatch && iid!=events_iid) return E_NOINTERFACE;
        *out=static_cast<IDispatch*>(this); AddRef(); return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&references); }
    ULONG STDMETHODCALLTYPE Release() override { auto n=InterlockedDecrement(&references); if(!n) delete this; return n; } // ubs:ignore - COM Release frees the object only after its last reference.
    HRESULT STDMETHODCALLTYPE GetTypeInfoCount(UINT* count) override { if(!count) return E_POINTER; *count=0; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetTypeInfo(UINT,LCID,ITypeInfo**) override { return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE GetIDsOfNames(REFIID,LPOLESTR*,UINT,LCID,DISPID*) override { return E_NOTIMPL; }
    HRESULT STDMETHODCALLTYPE Invoke(DISPID id,REFIID iid,LCID,WORD flags,DISPPARAMS* params,VARIANT*,EXCEPINFO*,UINT*) override {
        if(iid!=IID_NULL || flags!=3 || !params) return failure=E_UNEXPECTED;
        if(id==0xa34) { ++calculations; return params->cArgs?failure=E_UNEXPECTED:S_OK; }
        if(id!=0x61c || params->cArgs!=2 || params->cNamedArgs!=2 || params->rgdispidNamedArgs[0]!=0 || params->rgdispidNamedArgs[1]!=1) return failure=E_UNEXPECTED;
        if(params->rgvarg[0].vt!=VT_DISPATCH || params->rgvarg[1].vt!=VT_DISPATCH) return failure=E_UNEXPECTED;
        auto sheet=params->rgvarg[0].pdispVal;
        Object* typed=nullptr;
        HRESULT hr=sheet->QueryInterface(worksheet_iid,reinterpret_cast<void**>(&typed));
        BSTR name=nullptr;
        if(SUCCEEDED(hr)) hr=reinterpret_cast<HRESULT(WINAPI*)(Object*,BSTR*)>(typed->vtable[18])(typed,&name);
        if(SUCCEEDED(hr) && (!name || wcscmp(name,L"Unit"))) hr=E_UNEXPECTED;
        SysFreeString(name); if(typed) release(typed);
        auto range=params->rgvarg[1].pdispVal;
        DISPPARAMS none{}; VARIANT address{};
        if(SUCCEEDED(hr)) hr=range->Invoke(0xec,IID_NULL,0,DISPATCH_PROPERTYGET,&none,&address,nullptr,nullptr);
        if(SUCCEEDED(hr) && (address.vt!=VT_BSTR || wcscmp(address.bstrVal,expected_address.c_str()))) hr=E_UNEXPECTED;
        VariantClear(&address);
        if(SUCCEEDED(hr))hr=range->Invoke(0x76,IID_NULL,0,DISPATCH_PROPERTYGET,&none,&address,nullptr,nullptr);
        if(SUCCEEDED(hr)&&(address.vt!=VT_I4||address.lVal!=expected_count))hr=E_UNEXPECTED;
        VariantClear(&address);
        VARIANT qualified[4]{};
        qualified[0].vt=VT_BOOL; qualified[0].boolVal=VARIANT_TRUE;
        qualified[1].vt=VT_I4; qualified[1].lVal=1;
        qualified[2].vt=qualified[3].vt=VT_BOOL; qualified[2].boolVal=qualified[3].boolVal=VARIANT_TRUE;
        DISPPARAMS external{qualified,nullptr,4,0};
        if(SUCCEEDED(hr) && range->Invoke(0xec,IID_NULL,0,DISPATCH_PROPERTYGET,&external,&address,nullptr,nullptr)!=E_NOTIMPL) hr=E_UNEXPECTED;
        VariantClear(&address);
        if(FAILED(hr)) return failure=hr;
        if(retained) retained->Release(); retained=range; retained->AddRef(); ++changes;
        if(remove_from) { hr=remove_from->Unadvise(cookie); remove_from=nullptr; if(FAILED(hr)) failure=hr; }
        return failure;
    }
};
}
extern "C" HRESULT dna_com_test_events() noexcept {
    auto projection=static_cast<Projection*>(dna_com_create(1,nullptr,nullptr,nullptr,nullptr));
    if(!projection) return E_OUTOFMEMORY;
    HRESULT hr=dna_com_sheet(projection,L"Unit",4);
    IConnectionPointContainer* container=nullptr;
    IConnectionPoint *first=nullptr,*second=nullptr;
    auto a=new(std::nothrow) EventProbe; auto b=new(std::nothrow) EventProbe;
    DWORD ca=0,cb=0;
    if(!a || !b) hr=E_OUTOFMEMORY;
    if(SUCCEEDED(hr)) hr=query(projection->app,IID_IConnectionPointContainer,reinterpret_cast<void**>(&container));
    if(SUCCEEDED(hr)) hr=container->FindConnectionPoint(events_iid,&first);
    if(SUCCEEDED(hr)) hr=container->FindConnectionPoint(events_iid,&second);
    if(SUCCEEDED(hr)) hr=first->Advise(a,&ca);
    if(SUCCEEDED(hr)) hr=second->Advise(b,&cb);
    if(SUCCEEDED(hr)) { a->remove_from=second; a->cookie=ca; hr=dna_com_sheet_change(projection,2,3); }
    if(SUCCEEDED(hr)) hr=dna_com_sheet_change(projection,2,3);
    if(SUCCEEDED(hr)) hr=dna_com_enable_events(projection,false);
    if(SUCCEEDED(hr)) hr=dna_com_sheet_change(projection,2,3);
    if(SUCCEEDED(hr)) hr=dna_com_after_calculate(projection);
    if(SUCCEEDED(hr)) hr=dna_com_enable_events(projection,true);
    if(SUCCEEDED(hr)) hr=dna_com_after_calculate(projection);
    if(SUCCEEDED(hr) && (a->changes!=1 || b->changes!=2 || a->calculations!=0 || b->calculations!=1)) hr=E_UNEXPECTED;
    if(SUCCEEDED(hr)) {
        Area areas[]={{1,1,2,2},{7,5,7,5}};
        b->expected_address=L"$B$2:$C$3,$F$8";b->expected_count=5;
        hr=dna_com_sheet_change_areas(projection,0,L"Unit",4,areas,2);
        if(SUCCEEDED(hr)&&b->changes!=3)hr=E_UNEXPECTED;
    }
    if(SUCCEEDED(hr)) hr=first->Unadvise(cb);
    if(SUCCEEDED(hr) && second->Unadvise(cb)!=CONNECT_E_NOCONNECTION) hr=E_UNEXPECTED;
    if(first) first->Release(); if(second) second->Release(); if(container) container->Release();
    dna_com_destroy(projection);
    if(SUCCEEDED(hr) && b->retained) {
        DISPPARAMS params{}; VARIANT result{};
        if(b->retained->Invoke(0xec,IID_NULL,0,DISPATCH_PROPERTYGET,&params,&result,nullptr,nullptr)!=CO_E_OBJNOTCONNECTED) hr=E_UNEXPECTED;
        VariantClear(&result);
    }
    if(a) a->Release(); if(b) b->Release(); return hr;
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
        else if(result.vt==VT_I4) { output->kind=1; output->number=result.lVal; }
        else if(result.vt==VT_BOOL) { output->kind=3; output->number=result.boolVal!=VARIANT_FALSE; }
        else hr=E_NOTIMPL;
    }
    VariantClear(&macro); VariantClear(&result); return hr;
}

extern "C" HRESULT dna_com_test_office() noexcept {
    auto projection=static_cast<Projection*>(dna_com_create(1,nullptr,nullptr,nullptr,nullptr));
    if(!projection)return E_OUTOFMEMORY;
    struct Owner {Projection* p;~Owner(){if(p)dna_com_destroy(p);}} owner{projection};
    struct Held {VARIANT value{};~Held(){VariantClear(&value);}} bars,bar,controls,popup,caption,count,addins,identity_book,identity_books,identity_name,identity_lookup;
    auto call=[](IDispatch* object,const wchar_t* name,WORD flags,VARIANT* arg,VARIANT* result) {
        LPOLESTR text=const_cast<LPOLESTR>(name);DISPID id;HRESULT hr=object->GetIDsOfNames(IID_NULL,&text,1,0,&id);
        if(FAILED(hr))return hr;DISPID put=DISPID_PROPERTYPUT;
        DISPPARAMS params{arg,(flags&DISPATCH_PROPERTYPUT)?&put:nullptr,arg?1u:0u,(flags&DISPATCH_PROPERTYPUT)?1u:0u};
        return object->Invoke(id,IID_NULL,0,flags,&params,result,nullptr,nullptr);
    };
    auto app=reinterpret_cast<IDispatch*>(projection->app);
    HRESULT hr=dna_com_workbook(projection,L"RTD Book2",9);if(FAILED(hr))return hr;
    hr=call(app,L"ActiveWorkbook",DISPATCH_PROPERTYGET,nullptr,&identity_book.value);if(FAILED(hr))return hr;
    hr=call(identity_book.value.pdispVal,L"Name",DISPATCH_PROPERTYGET,nullptr,&identity_name.value);if(FAILED(hr))return hr;
    if(identity_name.value.vt!=VT_BSTR || wcscmp(identity_name.value.bstrVal,L"RTD Book2"))return E_UNEXPECTED;
    hr=call(app,L"Workbooks",DISPATCH_PROPERTYGET,nullptr,&identity_books.value);if(FAILED(hr))return hr;
    VARIANT identity_key{};identity_key.vt=VT_BSTR;identity_key.bstrVal=SysAllocString(L"RTD Book2");
    if(!identity_key.bstrVal)return E_OUTOFMEMORY;
    hr=call(identity_books.value.pdispVal,L"Item",DISPATCH_PROPERTYGET,&identity_key,&identity_lookup.value);VariantClear(&identity_key);if(FAILED(hr))return hr;
    if(identity_lookup.value.vt!=VT_DISPATCH || identity_lookup.value.pdispVal!=identity_book.value.pdispVal)return E_UNEXPECTED;
    auto identity_thread=projection->app->state->thread;projection->app->state->thread=identity_thread+1;
    hr=dna_com_workbook(projection,L"WrongThread",11);projection->app->state->thread=identity_thread;
    if(hr!=RPC_E_WRONG_THREAD || projection->app->state->workbook_name!=L"RTD Book2")return E_UNEXPECTED;
    hr=call(app,L"COMAddIns",DISPATCH_PROPERTYGET,nullptr,&addins.value);if(FAILED(hr))return hr;
    using EarlyItem=HRESULT (WINAPI*)(IDispatch*,VARIANT*,IDispatch**);
    auto early_item=reinterpret_cast<EarlyItem>((*reinterpret_cast<void***>(addins.value.pdispVal))[9]);
    VARIANT probe{};probe.vt=VT_I4;probe.lVal=1;IDispatch* none=nullptr;
    auto thread=projection->app->state->thread;projection->app->state->thread=thread+1;
    hr=early_item(addins.value.pdispVal,&probe,&none);projection->app->state->thread=thread;if(hr!=RPC_E_WRONG_THREAD)return E_FAIL;
    hr=call(app,L"CommandBars",DISPATCH_PROPERTYGET,nullptr,&bars.value);if(FAILED(hr))return hr;
    VARIANT index{};index.vt=VT_I4;index.lVal=1;
    hr=call(bars.value.pdispVal,L"Item",DISPATCH_PROPERTYGET,&index,&bar.value);if(FAILED(hr))return hr;
    hr=call(bar.value.pdispVal,L"Controls",DISPATCH_PROPERTYGET,nullptr,&controls.value);if(FAILED(hr))return hr;
    index.lVal=10;hr=call(controls.value.pdispVal,L"Add",DISPATCH_METHOD,&index,&popup.value);if(FAILED(hr))return hr;
    IUnknown* queried=nullptr;hr=popup.value.pdispVal->QueryInterface(popup_iid,reinterpret_cast<void**>(&queried));if(FAILED(hr))return hr;queried->Release();
    if(popup.value.pdispVal->QueryInterface(button_iid,reinterpret_cast<void**>(&queried))!=E_NOINTERFACE)return E_FAIL;
    caption.value.vt=VT_BSTR;caption.value.bstrVal=SysAllocString(L"Private test menu");
    hr=call(popup.value.pdispVal,L"Caption",DISPATCH_PROPERTYPUT,&caption.value,nullptr);if(FAILED(hr))return hr;
    hr=call(controls.value.pdispVal,L"Count",DISPATCH_PROPERTYGET,nullptr,&count.value);if(FAILED(hr)||count.value.vt!=VT_I4||count.value.lVal!=1)return E_FAIL;
    VariantClear(&count.value);hr=call(popup.value.pdispVal,L"Delete",DISPATCH_METHOD,nullptr,nullptr);if(FAILED(hr))return hr;
    hr=call(controls.value.pdispVal,L"Count",DISPATCH_PROPERTYGET,nullptr,&count.value);if(FAILED(hr)||count.value.lVal!=0)return E_FAIL;
    VariantClear(&count.value);hr=call(popup.value.pdispVal,L"Caption",DISPATCH_PROPERTYGET,nullptr,&count.value);if(hr!=CO_E_OBJNOTCONNECTED)return E_FAIL;
    dna_com_destroy(projection);owner.p=nullptr;
    if(early_item(addins.value.pdispVal,&probe,&none)!=CO_E_OBJNOTCONNECTED)return E_FAIL;
    hr=call(controls.value.pdispVal,L"Count",DISPATCH_PROPERTYGET,nullptr,&count.value);
    return hr==CO_E_OBJNOTCONNECTED?S_OK:E_FAIL;
}

namespace {
class ReentrantRibbon final : public Extensibility,public Ribbon {
    LONG references=1;
    Object* item;
    int mode;
    int* live;
public:
    ReentrantRibbon(Object* o,int action,int* count):item(o),mode(action),live(count){++*live;}
    ~ReentrantRibbon(){--*live;}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void** out) override {
        if(!out)return E_POINTER;*out=nullptr;
        if(iid==ribbon_iid) {
            if(mode==3){HRESULT hr=connect_addin(item,false);if(FAILED(hr))return hr;}
            *out=static_cast<Ribbon*>(this);
        }else if(iid==IID_IUnknown||iid==IID_IDispatch||iid==extensibility_iid)*out=static_cast<Extensibility*>(this);
        else return E_NOINTERFACE;
        AddRef();return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override{return InterlockedIncrement(&references);}
    ULONG STDMETHODCALLTYPE Release() override{auto n=InterlockedDecrement(&references);if(!n)delete this;return n;} // ubs:ignore - COM Release frees the object only after its last reference.
    HRESULT STDMETHODCALLTYPE GetTypeInfoCount(UINT* n) override{if(!n)return E_POINTER;*n=0;return S_OK;}
    HRESULT STDMETHODCALLTYPE GetTypeInfo(UINT,LCID,ITypeInfo**) override{return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE GetIDsOfNames(REFIID,LPOLESTR*,UINT,LCID,DISPID*) override{return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE Invoke(DISPID,REFIID,LCID,WORD,DISPPARAMS*,VARIANT*,EXCEPINFO*,UINT*) override{return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE OnConnection(IDispatch*,int,IDispatch*,SAFEARRAY**) override{
        if(mode!=5)return S_OK;
        VARIANT object{};object.vt=VT_DISPATCH;object.pdispVal=static_cast<Extensibility*>(this);
        DISPID named=DISPID_PROPERTYPUT;DISPPARAMS params{&object,&named,1,1};
        return office_invoke(item,7,DISPATCH_PROPERTYPUT,&params,nullptr);
    }
    HRESULT STDMETHODCALLTYPE OnDisconnection(int,SAFEARRAY**) override{
        if(mode==4)return refresh_addins(item->parent)==CO_E_OBJNOTCONNECTED?S_OK:E_FAIL;
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE OnAddInsUpdate(SAFEARRAY**) override{return S_OK;}
    HRESULT STDMETHODCALLTYPE OnStartupComplete(SAFEARRAY**) override{return S_OK;}
    HRESULT STDMETHODCALLTYPE OnBeginShutdown(SAFEARRAY**) override{return S_OK;}
    HRESULT STDMETHODCALLTYPE GetCustomUI(BSTR,BSTR* out) override {
        if(!out)return E_POINTER;*out=nullptr;
        if(mode==1)for(int i=0;i<32;++i) {
            auto child=new_office(item->application,item->parent,OfficeRole::Addin);
            child->com_addin=std::make_shared<ComAddinState>();item->parent->children.push_back(child);
        }
        if(mode==2){HRESULT hr=connect_addin(item,false);if(FAILED(hr))return hr;}
        *out=SysAllocString(L"<customUI/>");return *out?S_OK:E_OUTOFMEMORY;
    }
};
}
extern "C" HRESULT dna_com_test_ribbon_reentry() noexcept {
    for(int mode=1;mode<=5;++mode) {
        auto projection=static_cast<Projection*>(dna_com_create(1,nullptr,nullptr,nullptr,nullptr));if(!projection)return E_OUTOFMEMORY;
        auto app=projection->app;app->state->addins=new_office(app,app,OfficeRole::Addins);
        auto item=new_office(app,app->state->addins,OfficeRole::Addin);app->state->addins->children.push_back(item);add_ref(item);
        int live=0;item->com_addin=std::make_shared<ComAddinState>();auto& data=*item->com_addin;
        data.extension=new ReentrantRibbon(item,mode,&live);data.connected=true;data.generation=1;
        wchar_t module[32768];DWORD length=GetModuleFileNameW(nullptr,module,32768);data.module.assign(module,length);
        HRESULT hr;
        if(mode==5){data.transition=true;SAFEARRAY* custom=nullptr;hr=data.extension->OnConnection(nullptr,0,nullptr,&custom);data.transition=false;}
        else hr=mode==4?close_office(app):read_ribbon_ui(app);
        bool good=mode==5?SUCCEEDED(hr)&&data.public_object==static_cast<IDispatch*>(data.extension):mode==1?SUCCEEDED(hr)&&data.ui_requested&&data.xml==L"<customUI/>":mode==4?SUCCEEDED(hr)&&live==0:hr==E_ABORT&&!data.ui_requested&&data.xml.empty()&&live==0;
        HRESULT closed=close_office(app);if(FAILED(closed)||live!=0)good=false;
        release(item);dna_com_destroy(projection);if(!good)return E_FAIL;
    }
    return S_OK;
}

// Values-only profile: Application.Caller and Application.ThisCell are refused with Excel's 1004
// and "needs a workbook: open in DNA Calc" (dc-svqh).
extern "C" HRESULT dna_com_test_refusals() noexcept {
    auto projection=static_cast<Projection*>(dna_com_create(1,nullptr,nullptr,nullptr,nullptr));
    if(!projection)return E_OUTOFMEMORY;
    struct Owner {Projection* p;~Owner(){if(p)dna_com_destroy(p);}} owner{projection};
    auto app=reinterpret_cast<IDispatch*>(projection->app);
    for(const wchar_t* member:{L"Caller",L"ThisCell"}) {
        LPOLESTR text=const_cast<LPOLESTR>(member);DISPID id;
        HRESULT hr=app->GetIDsOfNames(IID_NULL,&text,1,0,&id);if(FAILED(hr))return hr;
        DISPPARAMS params{};VARIANT result{};EXCEPINFO error{};
        hr=app->Invoke(id,IID_NULL,0,DISPATCH_PROPERTYGET,&params,&result,&error,nullptr);
        bool ok=hr==DISP_E_EXCEPTION && error.scode==static_cast<SCODE>(0x800A03ECu) && error.bstrDescription
            && !wcscmp(error.bstrDescription,L"needs a workbook: open in DNA Calc");
        SysFreeString(error.bstrSource);SysFreeString(error.bstrDescription);
        if(!ok)return E_UNEXPECTED;
    }
    return S_OK;
}


// The same bridge is exercised by the DNA Calc page; this test checks COM ABI ownership
// and direct/vtable metadata access without Excel or a registered class.
extern "C" HRESULT dna_com_test_workbook_bridge() noexcept {
    auto projection=static_cast<Projection*>(dna_com_create(1,nullptr,nullptr,nullptr,nullptr));
    if(!projection)return E_OUTOFMEMORY;
    struct Owner { Projection* p;~Owner(){dna_com_destroy(p);} } owner{projection};
    struct Book { const wchar_t* full=L"C:\\Models\\Named.xlsx";const wchar_t* sheet=L"Second";bool enabled=true; } book;
    Bridge bridge=+[](const void* context,unsigned operation,Value* output)->HRESULT {
        auto book=const_cast<Book*>(static_cast<const Book*>(context));
        if(operation==2){output->number=book->enabled;return S_OK;}
        if(operation==3){book->enabled=output->number!=0;return S_OK;}
        const auto text=operation==0?book->full:book->sheet;
        output->length=wcslen(text);output->text=SysAllocString(text);return output->text?S_OK:E_OUTOFMEMORY;
    };
    HRESULT hr=dna_com_bridge(projection,&book,bridge);if(FAILED(hr))return hr;
    BSTR text=nullptr;
    hr=get_name(projection->workbook,&text);bool good=SUCCEEDED(hr)&&!wcscmp(text,L"Named.xlsx");SysFreeString(text);if(!good)return E_FAIL;
    hr=get_path(projection->workbook,0,&text);good=SUCCEEDED(hr)&&!wcscmp(text,L"C:\\Models");SysFreeString(text);if(!good)return E_FAIL;
    hr=get_fullname(projection->workbook,0,&text);good=SUCCEEDED(hr)&&!wcscmp(text,book.full);SysFreeString(text);if(!good)return E_FAIL;
    book.full=L"D:\\Other\\Saved.xlsx";
    DISPPARAMS none{};VARIANT result{};
    hr=invoke(projection->workbook,0x121,IID_NULL,0,DISPATCH_PROPERTYGET,&none,&result,nullptr,nullptr);
    good=SUCCEEDED(hr)&&result.vt==VT_BSTR&&!wcscmp(result.bstrVal,book.full);VariantClear(&result);if(!good)return E_FAIL;
    IDispatch* sheet=nullptr;hr=get_active_sheet(projection->app,&sheet);if(FAILED(hr))return hr;
    hr=get_name(reinterpret_cast<Object*>(sheet),&text);sheet->Release();good=SUCCEEDED(hr)&&!wcscmp(text,L"Second");SysFreeString(text);if(!good)return E_FAIL;
    hr=put_enable_events(projection->app,VARIANT_FALSE);if(FAILED(hr)||book.enabled)return E_FAIL;
    book.enabled=true;VARIANT_BOOL enabled=VARIANT_FALSE;hr=get_enable_events(projection->app,&enabled);
    return SUCCEEDED(hr)&&enabled==VARIANT_TRUE?S_OK:E_FAIL;
}

// ABI control for the workbook range callbacks. The real Rust/session route is tested
// by calc-host's tracked view controls and the Windows page's generated XLL cases.
extern "C" HRESULT dna_com_test_ranges() noexcept {
    auto projection=static_cast<Projection*>(dna_com_create(1,nullptr,nullptr,nullptr,nullptr));
    if(!projection)return E_OUTOFMEMORY;
    struct Owner {Projection* p;~Owner(){dna_com_destroy(p);}} owner{projection};
    struct Handle {std::vector<Area> areas;unsigned long long sheet;};
    Bridge bridge=+[](const void*,unsigned operation,Value* value)->HRESULT {
        if(operation==2){value->number=1;return S_OK;}
        if(operation==3)return S_OK;
        const wchar_t* text=operation==0?L"C:\\Models\\Named.xlsx":L"Unit";
        value->text=SysAllocString(text);value->length=wcslen(text);return value->text?S_OK:E_OUTOFMEMORY;
    };
    RangeApi api=+[](const void*,unsigned operation,void* token,const wchar_t* address,size_t length,RangeSpec* output)->HRESULT {
        Handle* handle=nullptr;
        std::unique_ptr<Handle> owned;
        try {
            if(operation==2)handle=static_cast<Handle*>(token);
            else if(operation==1)handle=new Handle{{{2,1,2,1}},7};
            else if(operation==3)handle=new Handle{std::vector<Area>(output->areas,output->areas+output->count),output->sheet};
            else if(operation==0){
                auto text=std::wstring(address,length);
                if(text==L"A1:B2")handle=new Handle{{{0,0,1,1}},7};
                else if(text==L"D5")handle=new Handle{{{4,3,4,3}},7};
                else if(text==L"Other!A1")handle=new Handle{{{0,0,0,0}},8};
                else return static_cast<HRESULT>(0x800A03ECu);
            }
            if(!handle)return E_INVALIDARG;
            if(operation!=2)owned.reset(handle);
            output->name=SysAllocString(handle->sheet==7?L"Unit":L"Other");
            if(!output->name)return E_OUTOFMEMORY;
            output->handle=handle;output->sheet=handle->sheet;output->areas=handle->areas.data();output->count=handle->areas.size();
            owned.release();return S_OK;
        }catch(...){return E_OUTOFMEMORY;}
    };
    RangeFree free=+[](void* token){std::unique_ptr<Handle> owned(static_cast<Handle*>(token));};
    RangeRead read=+[](const void*,void*,bool,void* output)->HRESULT {
        // A non-square SAFEARRAY catches swapped row/column bounds and element order.
        Value values[6]{};for(unsigned i=0;i<6;++i){values[i].kind=1;values[i].number=10+i;}
        return dna_com_values(output,values,2,3);
    };
    HRESULT hr=dna_com_bridge(projection,nullptr,bridge);if(FAILED(hr))return hr;
    hr=dna_com_ranges(projection,api,read,free);if(FAILED(hr))return hr;
    auto app=projection->app;
    VARIANT argument{};argument.vt=VT_BSTR;argument.bstrVal=SysAllocString(L"A1:B2");
    DISPPARAMS params{&argument,nullptr,1,0};VARIANT result{};
    hr=invoke(app,0xc5,IID_NULL,0,DISPATCH_PROPERTYGET,&params,&result,nullptr,nullptr);VariantClear(&argument);
    if(FAILED(hr)||result.vt!=VT_DISPATCH)return E_FAIL;
    auto range=result.pdispVal;DISPPARAMS none{};VARIANT value{};
    hr=range->Invoke(0xec,IID_NULL,0,DISPATCH_PROPERTYGET,&none,&value,nullptr,nullptr);
    bool good=SUCCEEDED(hr)&&value.vt==VT_BSTR&&!wcscmp(value.bstrVal,L"$A$1:$B$2");VariantClear(&value);
    if(good){hr=range->Invoke(0x76,IID_NULL,0,DISPATCH_PROPERTYGET,&none,&value,nullptr,nullptr);good=SUCCEEDED(hr)&&value.vt==VT_I4&&value.lVal==4;VariantClear(&value);}
    if(good){hr=range->Invoke(0x6,IID_NULL,0,DISPATCH_PROPERTYGET,&none,&value,nullptr,nullptr);
        LONG low=0,high=0;good=SUCCEEDED(hr)&&value.vt==(VT_ARRAY|VT_VARIANT)&&SafeArrayGetDim(value.parray)==2;
        if(good){SafeArrayGetLBound(value.parray,1,&low);SafeArrayGetUBound(value.parray,1,&high);good=low==1&&high==2;}
        if(good){SafeArrayGetLBound(value.parray,2,&low);SafeArrayGetUBound(value.parray,2,&high);good=low==1&&high==3;}
        if(good){LONG at[]={2,3};VARIANT last{};hr=SafeArrayGetElement(value.parray,at,&last);good=SUCCEEDED(hr)&&last.vt==VT_R8&&last.dblVal==15;VariantClear(&last);}VariantClear(&value);}
    if(good){UINT count=0;ITypeInfo* info=nullptr;good=SUCCEEDED(range->GetTypeInfoCount(&count))&&count==1&&SUCCEEDED(range->GetTypeInfo(0,0,&info));
        if(info){BSTR name=nullptr;hr=info->GetDocumentation(MEMBERID_NIL,&name,nullptr,nullptr,nullptr);good=good&&SUCCEEDED(hr)&&!wcscmp(name,L"Range");SysFreeString(name);info->Release();}}
    VARIANT caller{};
    if(good){hr=invoke(app,0x13d,IID_NULL,0,DISPATCH_PROPERTYGET,&none,&caller,nullptr,nullptr);good=SUCCEEDED(hr)&&caller.vt==VT_DISPATCH;}
    if(good){hr=caller.pdispVal->Invoke(0x101,IID_NULL,0,DISPATCH_PROPERTYGET,&none,&value,nullptr,nullptr);good=SUCCEEDED(hr)&&value.vt==VT_I4&&value.lVal==3;VariantClear(&value);}
    if(good){VARIANT args[]={caller,result};DISPPARAMS u{args,nullptr,2,0};VARIANT united{};hr=invoke(app,0x30b,IID_NULL,0,DISPATCH_METHOD,&u,&united,nullptr,nullptr);
        if(SUCCEEDED(hr)){hr=united.pdispVal->Invoke(0x76,IID_NULL,0,DISPATCH_PROPERTYGET,&none,&value,nullptr,nullptr);good=SUCCEEDED(hr)&&value.vt==VT_I4&&value.lVal==5;VariantClear(&value);}else good=false;VariantClear(&united);}
    if(good){hr=invoke(app,0x7aa,IID_NULL,0,DISPATCH_PROPERTYGET,&none,&value,nullptr,nullptr);good=SUCCEEDED(hr)&&value.vt==VT_DISPATCH;VariantClear(&value);}
    VariantClear(&caller);VariantClear(&result);return good?S_OK:E_FAIL;
}
