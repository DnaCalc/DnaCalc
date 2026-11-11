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
using OwnsModule = bool (WINAPI*)(const void*,const wchar_t*,size_t);
// The cell the running worksheet call was entered for: zero-based row and column; S_FALSE when no cell.
using CallerOf = HRESULT (WINAPI*)(const void*,unsigned*,unsigned*);
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
    CallerOf caller_of = nullptr;
    bool connected = true;
    bool enable_events = true;
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
// A zero-based inclusive rectangle of the one worksheet.
struct Rect { unsigned r1,c1,r2,c2; };
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
    bool deleted = false;
    std::shared_ptr<ComAddinState> com_addin;
    // A range's areas (empty: the single cell row,column) and what Rows/Columns made it: 0 the
    // range itself, 1 its Rows, 2 its Columns. Count of a Rows/Columns view is that dimension of
    // the first area, as Excel's is.
    std::vector<Rect> areas;
    int view = 0;
};
bool retain_office_owner(Object*);
ULONG WINAPI add_ref(Object* object) { return InterlockedIncrement(&object->references); }
ULONG WINAPI release(Object* object) {
    LONG count = InterlockedDecrement(&object->references);
    if (!count) {
        if(retain_office_owner(object))return InterlockedIncrement(&object->references);
        for(auto child:object->children) release(child);
        if(object->controls) release(object->controls);
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
    if(object->office!=OfficeRole::None&&object->deleted)return CO_E_OBJNOTCONNECTED;
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
HRESULT WINAPI object_type_count(Object* object,UINT* count) { if(!count) return E_POINTER; *count=object->is_range&&range_type_info()?1:0; return S_OK; }
HRESULT WINAPI object_type_info(Object* object,UINT index,LCID,ITypeInfo** info) {
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
        else if(object->is_application && !_wcsicmp(names[i],L"Range")) ids[i]=0xc5;
        else if(object->is_application && !_wcsicmp(names[i],L"Caller")) ids[i]=0x13d;
        else if(object->is_application && !_wcsicmp(names[i],L"Union")) ids[i]=0x30b;
        else if(object->is_application && !_wcsicmp(names[i],L"COMAddIns")) ids[i]=0x704;
        else if(object->is_application && !_wcsicmp(names[i],L"CommandBars")) ids[i]=0x59f;
        else if(object->is_application && !_wcsicmp(names[i],L"AutomationSecurity")) ids[i]=0x795;
        else if((object->is_application || object->is_workbook) && !_wcsicmp(names[i],L"ActiveSheet")) ids[i]=0x133;
        else if(object->is_application && !_wcsicmp(names[i],L"ActiveWorkbook")) ids[i]=0x134;
        else if((object->is_workbook || object->is_worksheet) && !_wcsicmp(names[i],L"Name")) ids[i]=0x6e;
        else if(object->is_workbook && !_wcsicmp(names[i],L"FullName")) ids[i]=0x121;
        else if(object->is_collection && !_wcsicmp(names[i],L"Count")) ids[i]=0x76;
        else if(object->is_collection && !_wcsicmp(names[i],L"Item")) ids[i]=0xaa;
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
    *out=SysAllocString((object->is_worksheet?object->state->worksheet_name:object->state->workbook_name).c_str()); return *out?S_OK:E_OUTOFMEMORY;
}
HRESULT WINAPI get_active_sheet(Object* object,IDispatch** out) {
    if(!out) return E_POINTER;
    *out=nullptr; HRESULT hr=valid(object); if(FAILED(hr)) return hr;
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
std::vector<Rect> areas_of(const Object* object) {
    if(!object->areas.empty()) return object->areas;
    return {Rect{object->row,object->column,object->row,object->column}};
}
std::wstring column_letters(unsigned column) {
    std::wstring letters; unsigned n=column+1;
    while(n) { letters.insert(letters.begin(),static_cast<wchar_t>(L'A'+(n-1)%26)); n=(n-1)/26; }
    return letters;
}
// What a Range's Count answers: the cells of every area; a Rows or Columns view counts its
// dimension of the first area, as Excel does.
LONG range_count(const Object* object) {
    auto areas=areas_of(object);
    if(object->view==1) return static_cast<LONG>(areas[0].r2-areas[0].r1+1);
    if(object->view==2) return static_cast<LONG>(areas[0].c2-areas[0].c1+1);
    unsigned long long cells=0;
    for(auto& a:areas) cells+=static_cast<unsigned long long>(a.r2-a.r1+1)*(a.c2-a.c1+1);
    return cells>0x7fffffffULL?0x7fffffff:static_cast<LONG>(cells);
}
HRESULT range_address(Object* object,DISPPARAMS* params,VARIANT* result) {
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
    auto areas=areas_of(object);
    std::wstring text;
    if(external) {
        if(areas.size()!=1) return E_NOTIMPL;
        auto& state=*object->state;
        if(!state.qualifier) return E_NOTIMPL;
        BSTR prefix=nullptr;
        HRESULT hr=state.qualifier(state.workbook_name.data(),state.workbook_name.size(),state.worksheet_name.data(),state.worksheet_name.size(),&prefix);
        if(FAILED(hr)) return hr;
        text.assign(prefix,SysStringLen(prefix)); SysFreeString(prefix); text+=L"!";
    }
    for(size_t k=0;k<areas.size();++k) {
        auto& a=areas[k];
        bool single=a.r1==a.r2 && a.c1==a.c2;
        if(k) text+=L",";
        if(style==1) {
            auto cell=[&](unsigned row,unsigned column) {
                std::wstring t;
                if(absolute_column) t+=L"$";
                t+=column_letters(column);
                if(absolute_row) t+=L"$";
                return t+std::to_wstring(row+1);
            };
            text+=cell(a.r1,a.c1);
            if(!single) text+=L":"+cell(a.r2,a.c2);
        } else if(style==-4150 && absolute_row && absolute_column) {
            text+=L"R"+std::to_wstring(a.r1+1)+L"C"+std::to_wstring(a.c1+1);
            if(!single) text+=L":R"+std::to_wstring(a.r2+1)+L"C"+std::to_wstring(a.c2+1);
        } else return E_NOTIMPL;
    }
    if(result) { result->vt=VT_BSTR; result->bstrVal=SysAllocString(text.c_str()); if(!result->bstrVal) return E_OUTOFMEMORY; }
    return S_OK;
}
// Excel's application-defined error 1004 (0x800A03EC) for an address that names nothing here.
HRESULT application_defined(EXCEPINFO* error,const wchar_t* member) {
    if(error) {
        *error=EXCEPINFO{};
        error->scode=static_cast<SCODE>(0x800A03ECu);
        error->bstrSource=SysAllocString(L"Microsoft Excel");
        error->bstrDescription=SysAllocString((std::wstring(L"Method '")+member+L"' of object '_Global' failed").c_str());
    }
    return DISP_E_EXCEPTION;
}
bool parse_cell(const std::wstring& text,size_t& at,unsigned& row,unsigned& column) {
    size_t n=text.size();
    if(at<n && text[at]==L'$') ++at;
    unsigned long c=0; size_t letters=0;
    while(at<n && ((text[at]>=L'A'&&text[at]<=L'Z')||(text[at]>=L'a'&&text[at]<=L'z'))) {
        c=c*26+static_cast<unsigned long>(towupper(text[at])-L'A'+1); ++at;
        if(++letters>3) return false;
    }
    if(!letters || c>16384) return false;
    if(at<n && text[at]==L'$') ++at;
    unsigned long r=0; size_t digits=0;
    while(at<n && text[at]>=L'0'&&text[at]<=L'9') { r=r*10+static_cast<unsigned long>(text[at]-L'0'); ++at; if(++digits>7) return false; }
    if(!digits || r<1 || r>1048576) return false;
    row=static_cast<unsigned>(r-1); column=static_cast<unsigned>(c-1); return true;
}
bool same_name(const std::wstring& a,const std::wstring& b) { return a.size()==b.size() && !_wcsicmp(a.c_str(),b.c_str()); }
// "A1", "$A$1:$B$2", "A1,C3" and the same behind a sheet qualifier ('[Book1]X0'! or X0!). The
// one worksheet of this projection is the only sheet there is.
HRESULT parse_areas(State& state,const std::wstring& text,std::vector<Rect>& areas,EXCEPINFO* error) {
    std::wstring prefix,body=text;
    if(!body.empty() && body[0]==L'\'') {
        size_t i=1;
        for(;i<body.size();++i) {
            if(body[i]==L'\'') { if(i+1<body.size() && body[i+1]==L'\'') { prefix+=L'\''; ++i; continue; } break; }
            prefix+=body[i];
        }
        if(i+1>=body.size() || body[i]!=L'\'' || body[i+1]!=L'!') return application_defined(error,L"Range");
        body=body.substr(i+2);
    } else if(auto bang=body.find(L'!'); bang!=std::wstring::npos) {
        prefix=body.substr(0,bang); body=body.substr(bang+1);
    }
    if(!prefix.empty()) {
        bool known=same_name(prefix,state.worksheet_name) || same_name(prefix,L"["+state.workbook_name+L"]"+state.worksheet_name);
        if(!known && state.qualifier) {
            BSTR qualified=nullptr;
            if(SUCCEEDED(state.qualifier(state.workbook_name.data(),state.workbook_name.size(),state.worksheet_name.data(),state.worksheet_name.size(),&qualified)) && qualified) {
                known=same_name(prefix,std::wstring(qualified,SysStringLen(qualified)));
                SysFreeString(qualified);
            }
        }
        if(!known) return application_defined(error,L"Range");
    }
    size_t at=0;
    while(true) {
        while(at<body.size() && body[at]==L' ') ++at;
        Rect rect{};
        if(!parse_cell(body,at,rect.r1,rect.c1)) return application_defined(error,L"Range");
        rect.r2=rect.r1; rect.c2=rect.c1;
        if(at<body.size() && body[at]==L':') {
            ++at;
            if(!parse_cell(body,at,rect.r2,rect.c2)) return application_defined(error,L"Range");
            if(rect.r2<rect.r1) std::swap(rect.r1,rect.r2);
            if(rect.c2<rect.c1) std::swap(rect.c1,rect.c2);
        }
        areas.push_back(rect);
        while(at<body.size() && body[at]==L' ') ++at;
        if(at==body.size()) return S_OK;
        if(body[at]!=L',') return application_defined(error,L"Range");
        ++at;
    }
}
Object* make_range(Object* app,std::vector<Rect> areas,int view);
Object* own_range(IUnknown* candidate,const std::shared_ptr<State>& state);
HRESULT cell_argument(Object* app,const VARIANT& argument,std::vector<Rect>& areas,EXCEPINFO* error) {
    VARIANT value{}; HRESULT hr=VariantCopyInd(&value,const_cast<VARIANT*>(&argument)); if(FAILED(hr)) return hr;
    if(value.vt==VT_BSTR && value.bstrVal) hr=parse_areas(*app->state,std::wstring(value.bstrVal,SysStringLen(value.bstrVal)),areas,error);
    else if(value.vt==VT_DISPATCH) { auto range=own_range(value.pdispVal,app->state); if(range) areas=areas_of(range); else hr=DISP_E_TYPEMISMATCH; }
    else hr=DISP_E_TYPEMISMATCH;
    VariantClear(&value); return hr;
}
HRESULT give_range(Object* range,VARIANT* result) {
    if(!range) return E_OUTOFMEMORY;
    if(result) { result->vt=VT_DISPATCH; result->pdispVal=reinterpret_cast<IDispatch*>(range); } else release(range);
    return S_OK;
}
// Application.Range(Cell1, [Cell2]).
HRESULT application_range(Object* app,DISPPARAMS* params,VARIANT* result,EXCEPINFO* error) {
    if(params->cNamedArgs) return DISP_E_NONAMEDARGS;
    if(params->cArgs<1 || params->cArgs>2) return DISP_E_BADPARAMCOUNT;
    std::vector<Rect> areas,second;
    HRESULT hr=cell_argument(app,params->rgvarg[params->cArgs-1],areas,error); if(FAILED(hr)) return hr;
    if(params->cArgs==2 && !(params->rgvarg[0].vt==VT_ERROR && params->rgvarg[0].scode==DISP_E_PARAMNOTFOUND)) {
        hr=cell_argument(app,params->rgvarg[0],second,error); if(FAILED(hr)) return hr;
        if(areas.size()!=1 || second.size()!=1) return application_defined(error,L"Range");
        Rect box{(std::min)(areas[0].r1,second[0].r1),(std::min)(areas[0].c1,second[0].c1),(std::max)(areas[0].r2,second[0].r2),(std::max)(areas[0].c2,second[0].c2)};
        areas={box};
    }
    return give_range(make_range(app,std::move(areas),0),result);
}
// Application.Union(Arg1, Arg2, ...): the areas of every range, in order.
HRESULT application_union(Object* app,DISPPARAMS* params,VARIANT* result) {
    if(params->cNamedArgs) return DISP_E_NONAMEDARGS;
    if(params->cArgs<2 || params->cArgs>30) return DISP_E_BADPARAMCOUNT;
    std::vector<Rect> areas;
    for(UINT i=params->cArgs;i-->0;) {
        auto& argument=params->rgvarg[i];
        if(argument.vt==VT_ERROR && argument.scode==DISP_E_PARAMNOTFOUND) continue;
        VARIANT value{}; HRESULT hr=VariantCopyInd(&value,&argument); if(FAILED(hr)) return hr;
        auto range=value.vt==VT_DISPATCH?own_range(value.pdispVal,app->state):nullptr;
        if(!range) { VariantClear(&value); return DISP_E_TYPEMISMATCH; }
        for(auto& rect:areas_of(range)) areas.push_back(rect);
        VariantClear(&value);
    }
    return give_range(make_range(app,std::move(areas),0),result);
}
// Application.Caller: the cell the running worksheet call was entered for, as a Range, or the
// #REF! error Excel answers when no cell is calling. Only the cell is modelled, not CSE, spill
// or name callers (the Index argument is refused).
HRESULT application_caller(Object* app,DISPPARAMS* params,VARIANT* result) {
    if(params->cNamedArgs) return DISP_E_NONAMEDARGS;
    for(UINT i=0;i<params->cArgs;++i) if(!(params->rgvarg[i].vt==VT_ERROR && params->rgvarg[i].scode==DISP_E_PARAMNOTFOUND)) return DISP_E_BADPARAMCOUNT;
    unsigned row=0,column=0;
    auto& state=*app->state;
    HRESULT found=state.caller_of?state.caller_of(state.context,&row,&column):S_FALSE;
    if(FAILED(found)) return found;
    if(found!=S_OK) { if(result) { result->vt=VT_ERROR; result->scode=static_cast<SCODE>(0x800A07E7u); } return S_OK; }
    return give_range(make_range(app,{Rect{row,column,row,column}},0),result);
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
            else if(output.kind==4) { result->vt=VT_I4; result->lVal=static_cast<LONG>(output.number); }
            else if(output.kind==3) { result->vt=VT_BOOL; result->boolVal=output.number?VARIANT_TRUE:VARIANT_FALSE; }
            else hr=E_NOTIMPL;
        }
    }
    for(auto& v:indirect) VariantClear(&v);
    VariantClear(&macro);
    return hr;
}
HRESULT WINAPI invoke(Object* object,DISPID id,REFIID iid,LCID,WORD flags,DISPPARAMS* params,VARIANT* result,EXCEPINFO* excep,UINT*) {
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
    if(object->is_application && id==0xc5 && (flags&(DISPATCH_METHOD|DISPATCH_PROPERTYGET))) return application_range(object,params,result,excep);
    if(object->is_application && id==0x30b && (flags&DISPATCH_METHOD)) return application_union(object,params,result);
    if(object->is_application && id==0x13d && (flags&DISPATCH_PROPERTYGET)) return application_caller(object,params,result);
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
    if(object->is_range && id==0xec) return range_address(object,params,result);
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
        IDispatch* value=nullptr; hr=id==0x134?get_active_workbook(object,&value):get_active_sheet(object,&value);
        if(SUCCEEDED(hr)) { if(result) { result->vt=VT_DISPATCH; result->pdispVal=value; } else value->Release(); } return hr;
    }
    if(object->is_range && (id==0x101 || id==0xf0 || id==0x76)) {
        auto first=areas_of(object)[0];
        if(result) { result->vt=VT_I4; result->lVal=id==0x101?static_cast<LONG>(first.r1+1):(id==0xf0?static_cast<LONG>(first.c1+1):range_count(object)); } return S_OK;
    }
    if(object->is_range && (id==0x102 || id==0xf1)) {
        return give_range(make_range(object->application,areas_of(object),id==0x102?1:2),result);
    }
    if(id==0x76 && object->is_collection) { LONG count; hr=get_count(object,&count); if(SUCCEEDED(hr) && result) { result->vt=VT_I4; result->lVal=count; } return hr; }
    if((id==0x6e && (object->is_workbook || object->is_worksheet)) || (id==0x121 && object->is_workbook)) {
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
HRESULT WINAPI early_range(Object* object,VARIANT cell1,VARIANT cell2,IDispatch** out) {
    if(!out) return E_POINTER;
    *out=nullptr;
    VARIANT args[]={cell2,cell1};
    DISPPARAMS params{args,nullptr,2,0};
    VARIANT result{}; EXCEPINFO error{};
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    hr=application_range(object,&params,&result,&error);
    SysFreeString(error.bstrSource); SysFreeString(error.bstrDescription);
    // An early-bound call has no EXCEPINFO: Excel returns the error's own HRESULT (0x800A03EC).
    if(hr==DISP_E_EXCEPTION && error.scode) hr=error.scode;
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
    v[3]=reinterpret_cast<void*>(object_type_count); v[4]=reinterpret_cast<void*>(object_type_info);
    v[5]=reinterpret_cast<void*>(names); v[6]=reinterpret_cast<void*>(invoke);
    v[7]=reinterpret_cast<void*>(get_application); v[9]=reinterpret_cast<void*>(get_parent);
    v[223]=reinterpret_cast<void*>(get_path); v[300]=reinterpret_cast<void*>(get_enable_events); v[301]=reinterpret_cast<void*>(put_enable_events);
    v[52]=reinterpret_cast<void*>(get_workbooks);
    v[16]=reinterpret_cast<void*>(get_active_sheet); v[18]=reinterpret_cast<void*>(get_active_workbook);
    v[40]=reinterpret_cast<void*>(early_range); v[50]=reinterpret_cast<void*>(early_union); v[74]=reinterpret_cast<void*>(early_caller);
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
Object* make_range(Object* app,std::vector<Rect> areas,int view) {
    auto state=app->state; auto sheet=state->worksheet;
    unsigned row=areas.empty()?0:areas[0].r1,column=areas.empty()?0:areas[0].c1;
    auto range=new(std::nothrow) Object{window_vtable.data(),1,state,app,false,sheet,false,false,false,true,row,column};
    if(!range) return nullptr;
    try { range->areas=std::move(areas); } catch(...) { delete range; return nullptr; }
    range->view=view;
    add_ref(app); if(sheet) add_ref(sheet);
    return range;
}
// Our own Range, or null: a foreign dispatch object is never cast.
Object* own_range(IUnknown* candidate,const std::shared_ptr<State>& state) {
    if(!candidate || *reinterpret_cast<void***>(candidate)!=window_vtable.data()) return nullptr;
    auto object=reinterpret_cast<Object*>(candidate);
    return object->is_range && object->state==state?object:nullptr;
}
#include "projection-office.h"
struct Projection { Object* app; Object* window; Object* workbook; Object* workbooks; Object* worksheet; };

HRESULT fire(Object* object,DISPID id,VARIANT* args,UINT count) {
    HRESULT hr=valid(object); if(FAILED(hr)) return hr;
    if(!object->state->enable_events) return S_OK;
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
extern "C" void dna_com_set_caller(void* pointer,CallerOf caller) noexcept {
    static_cast<Projection*>(pointer)->app->state->caller_of=caller;
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
    release(projection->window); release(projection->worksheet); release(projection->workbook); release(projection->workbooks); release(projection->app); delete projection;
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
extern "C" HRESULT dna_com_sheet_change(void* pointer,unsigned row,unsigned column) noexcept {
    auto projection=static_cast<Projection*>(pointer);
    if(row>=1048576 || column>=16384) return E_INVALIDARG;
    auto range=new(std::nothrow) Object{window_vtable.data(),1,projection->app->state,projection->app,false,projection->worksheet,false,false,false,true,row,column};
    if(!range) return E_OUTOFMEMORY;
    add_ref(projection->app); add_ref(projection->worksheet);
    VARIANT args[2]{}; args[0].vt=VT_DISPATCH; args[0].pdispVal=reinterpret_cast<IDispatch*>(projection->worksheet);
    args[1].vt=VT_DISPATCH; args[1].pdispVal=reinterpret_cast<IDispatch*>(range);
    HRESULT hr=fire(projection->app,0x61c,args,2); release(range); return hr;
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
    ULONG STDMETHODCALLTYPE Release() override { auto n=InterlockedDecrement(&references); if(!n) delete this; return n; }
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
        if(SUCCEEDED(hr) && (address.vt!=VT_BSTR || wcscmp(address.bstrVal,L"$D$3"))) hr=E_UNEXPECTED;
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
HRESULT WINAPI test_caller(const void*,unsigned* row,unsigned* column) { *row=1; *column=2; return S_OK; }
// Application.Range, Union and Caller and the metadata of the Range objects they return, through
// IDispatch and through the early-bound _Application slots (dc-6332.12).
extern "C" HRESULT dna_com_test_ranges() noexcept {
    auto projection=static_cast<Projection*>(dna_com_create(1,nullptr,nullptr,nullptr,nullptr));
    if(!projection) return E_OUTOFMEMORY;
    struct Owner { Projection* p; ~Owner() { if(p) dna_com_destroy(p); } } owner{projection};
    HRESULT hr=dna_com_sheet(projection,L"Unit",4);
    auto app=projection->app;
    auto call=[&](Object* object,DISPID id,WORD flags,std::vector<VARIANT> args,VARIANT* out,EXCEPINFO* error=nullptr) {
        std::reverse(args.begin(),args.end());
        DISPPARAMS params{args.data(),nullptr,static_cast<UINT>(args.size()),0};
        return invoke(object,id,IID_NULL,0,flags,&params,out,error,nullptr);
    };
    auto text=[](const wchar_t* value) { VARIANT v{}; v.vt=VT_BSTR; v.bstrVal=SysAllocString(value); return v; };
    auto object_of=[](VARIANT& v) { return v.vt==VT_DISPATCH?reinterpret_cast<Object*>(v.pdispVal):nullptr; };
    auto address_is=[&](VARIANT& range,const wchar_t* expected) {
        VARIANT address{}; HRESULT hr=call(object_of(range),0xec,DISPATCH_PROPERTYGET,{},&address);
        bool same=SUCCEEDED(hr) && address.vt==VT_BSTR && !wcscmp(address.bstrVal,expected);
        VariantClear(&address); return same;
    };
    auto long_is=[&](Object* object,DISPID id,LONG expected) {
        VARIANT value{}; HRESULT hr=call(object,id,DISPATCH_PROPERTYGET,{},&value);
        return SUCCEEDED(hr) && value.vt==VT_I4 && value.lVal==expected;
    };
    auto make=[&](const wchar_t* address,VARIANT* out) {
        auto argument=text(address); HRESULT hr=call(app,0xc5,DISPATCH_PROPERTYGET,{argument},out); VariantClear(&argument); return hr;
    };
    VARIANT block{},cell{},rows{},columns{},pair{},result{};
    if(SUCCEEDED(hr)) hr=make(L"'Unit'!$A$1:$B$3",&block);
    if(SUCCEEDED(hr) && (!object_of(block) || !address_is(block,L"$A$1:$B$3"))) hr=E_UNEXPECTED;
    // Count is the cells, Rows.Count and Columns.Count their dimensions, Row and Column the first cell.
    if(SUCCEEDED(hr) && !(long_is(object_of(block),0x76,6) && long_is(object_of(block),0x101,1) && long_is(object_of(block),0xf0,1))) hr=E_UNEXPECTED;
    if(SUCCEEDED(hr)) hr=call(object_of(block),0x102,DISPATCH_PROPERTYGET,{},&rows);
    if(SUCCEEDED(hr)) hr=call(object_of(block),0xf1,DISPATCH_PROPERTYGET,{},&columns);
    if(SUCCEEDED(hr) && !(object_of(rows) && object_of(columns) && long_is(object_of(rows),0x76,3) && long_is(object_of(columns),0x76,2))) hr=E_UNEXPECTED;
    if(SUCCEEDED(hr)) hr=make(L"c3",&cell);
    if(SUCCEEDED(hr) && (!address_is(cell,L"$C$3") || !long_is(object_of(cell),0x101,3) || !long_is(object_of(cell),0xf0,3))) hr=E_UNEXPECTED;
    // Range(Cell1, Cell2) is the bounding rectangle.
    if(SUCCEEDED(hr)) { auto a=text(L"D5"),b=text(L"B2"); hr=call(app,0xc5,DISPATCH_PROPERTYGET,{a,b},&pair); VariantClear(&a); VariantClear(&b); }
    if(SUCCEEDED(hr) && !address_is(pair,L"$B$2:$D$5")) hr=E_UNEXPECTED;
    // Union keeps every area, in order; Count adds them.
    VARIANT two{},united{};
    if(SUCCEEDED(hr)) hr=make(L"A1:A2",&two);
    if(SUCCEEDED(hr)) hr=call(app,0x30b,DISPATCH_METHOD,{two,cell},&united);
    if(SUCCEEDED(hr) && (!address_is(united,L"$A$1:$A$2,$C$3") || !long_is(object_of(united),0x76,3))) hr=E_UNEXPECTED;
    // An address that names nothing here is Excel's error 1004, with its EXCEPINFO.
    if(SUCCEEDED(hr)) {
        auto bad=text(L"Elsewhere!A1"); EXCEPINFO error{};
        HRESULT failed=call(app,0xc5,DISPATCH_PROPERTYGET,{bad},&result,&error);
        if(failed!=DISP_E_EXCEPTION || error.scode!=static_cast<SCODE>(0x800A03ECu)) hr=E_UNEXPECTED;
        SysFreeString(error.bstrSource); SysFreeString(error.bstrDescription); VariantClear(&bad);
    }
    if(SUCCEEDED(hr)) { auto bad=text(L"A0"); HRESULT failed=call(app,0xc5,DISPATCH_PROPERTYGET,{bad},&result); VariantClear(&bad); if(failed!=DISP_E_EXCEPTION) hr=E_UNEXPECTED; }
    // No cell is calling: #REF!. With a caller, the Range of its cell.
    VARIANT caller{},early{};
    if(SUCCEEDED(hr)) hr=call(app,0x13d,DISPATCH_PROPERTYGET,{},&caller);
    if(SUCCEEDED(hr) && (caller.vt!=VT_ERROR || caller.scode!=static_cast<SCODE>(0x800A07E7u))) hr=E_UNEXPECTED;
    VariantClear(&caller);
    if(SUCCEEDED(hr)) { dna_com_set_caller(projection,test_caller); hr=call(app,0x13d,DISPATCH_PROPERTYGET,{},&caller); }
    if(SUCCEEDED(hr) && (!address_is(caller,L"$C$2") || !long_is(object_of(caller),0x101,2))) hr=E_UNEXPECTED;
    // GetTypeInfo(0) names the type "Range", which is what VBA's TypeName reads.
    if(SUCCEEDED(hr)) {
        UINT kinds=0; ITypeInfo* info=nullptr; BSTR name=nullptr;
        auto range=object_of(block);
        if(FAILED(object_type_count(range,&kinds)) || kinds!=1 || FAILED(object_type_info(range,0,0,&info)) || !info) hr=E_UNEXPECTED;
        if(SUCCEEDED(hr)) hr=info->GetDocumentation(MEMBERID_NIL,&name,nullptr,nullptr,nullptr);
        if(SUCCEEDED(hr) && (!name || wcscmp(name,L"Range"))) hr=E_UNEXPECTED;
        SysFreeString(name); if(info) info->Release();
        UINT none=1;
        if(SUCCEEDED(hr) && (FAILED(object_type_count(app,&none)) || none!=0)) hr=E_UNEXPECTED;
    }
    // The early-bound _Application slots 40 (Range), 50 (Union) and 74 (Caller).
    if(SUCCEEDED(hr)) {
        auto range=reinterpret_cast<HRESULT(WINAPI*)(Object*,VARIANT,VARIANT,IDispatch**)>(app->vtable[40]);
        VARIANT missing{}; missing.vt=VT_ERROR; missing.scode=DISP_E_PARAMNOTFOUND;
        auto address=text(L"B2:C3"); IDispatch* made=nullptr;
        hr=range(app,address,missing,&made); VariantClear(&address);
        early.vt=VT_DISPATCH; early.pdispVal=made;
        if(SUCCEEDED(hr) && (!made || !address_is(early,L"$B$2:$C$3"))) hr=E_UNEXPECTED;
        VariantClear(&early);
        auto elsewhere=text(L"Elsewhere!A1"); IDispatch* none=nullptr;
        if(SUCCEEDED(hr) && range(app,elsewhere,missing,&none)!=static_cast<HRESULT>(0x800A03ECu)) hr=E_UNEXPECTED;
        VariantClear(&elsewhere);
    }
    if(SUCCEEDED(hr)) {
        auto early_caller_entry=reinterpret_cast<HRESULT(WINAPI*)(Object*,VARIANT,LONG,VARIANT*)>(app->vtable[74]);
        VARIANT missing{}; missing.vt=VT_ERROR; missing.scode=DISP_E_PARAMNOTFOUND;
        hr=early_caller_entry(app,missing,0x409,&early);
        if(SUCCEEDED(hr) && !address_is(early,L"$C$2")) hr=E_UNEXPECTED;
        VariantClear(&early);
    }
    if(SUCCEEDED(hr)) {
        // Slot 50: Union(Arg1, Arg2, Arg3..Arg30, lcid, RHS), the lcid before the return value.
        auto early_union_entry=reinterpret_cast<decltype(&early_union)>(app->vtable[50]);
        VARIANT missing{}; missing.vt=VT_ERROR; missing.scode=DISP_E_PARAMNOTFOUND;
        IDispatch* made=nullptr;
        hr=early_union_entry(app,object_of(two)?reinterpret_cast<IDispatch*>(object_of(two)):nullptr,reinterpret_cast<IDispatch*>(object_of(cell)),
            missing,missing,missing,missing,missing,missing,missing,missing,missing,missing,
            missing,missing,missing,missing,missing,missing,missing,missing,missing,missing,
            missing,missing,missing,missing,missing,missing,missing,missing,0x409,&made);
        early.vt=VT_DISPATCH; early.pdispVal=made;
        if(SUCCEEDED(hr) && (!made || !address_is(early,L"$A$1:$A$2,$C$3"))) hr=E_UNEXPECTED;
        VariantClear(&early);
    }
    VariantClear(&block); VariantClear(&cell); VariantClear(&rows); VariantClear(&columns); VariantClear(&pair);
    VariantClear(&two); VariantClear(&united); VariantClear(&caller); VariantClear(&result);
    return hr;
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
    ULONG STDMETHODCALLTYPE Release() override{auto n=InterlockedDecrement(&references);if(!n)delete this;return n;}
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
