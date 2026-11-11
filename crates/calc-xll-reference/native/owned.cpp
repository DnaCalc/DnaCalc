// SPDX-License-Identifier: MIT
// Native ownership fixture; documented x64 XLOPER12 wire layout, no SDK header.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>

struct alignas(8) Oper {
    union { double number; wchar_t* string; int32_t integer; unsigned char wire[24]; } value{};
    uint32_t type=0;
};
static_assert(offsetof(Oper,type)==24 && sizeof(Oper)==32,"x64 XLOPER12 layout");
using Callback=int(__stdcall*)(int,int,Oper**,Oper*);
static Callback excel;
static HMODULE module;
static Oper registration;
#ifdef DNA_X5_RTD
static bool rtdOpen(const std::wstring& file);
static void rtdClose();
static void rtdLog(const wchar_t*,LONG);
#endif
static std::wstring counted(const std::wstring& text){return std::wstring(1,(wchar_t)text.size())+text;}
static Oper textOper(std::wstring& text){Oper o;o.type=2;o.value.string=text.data();return o;}
static void freeResult(Oper& result){Oper* args[]={&result};excel(16384,1,args,nullptr);}

extern "C" __declspec(dllexport) Oper* __stdcall NativeOwned(){
    const wchar_t text[]=L"native owned";
    constexpr size_t length=(sizeof(text)/sizeof(wchar_t))-1;
    auto p=(Oper*)HeapAlloc(GetProcessHeap(),HEAP_ZERO_MEMORY,sizeof(Oper));
    if(!p)return nullptr;
    p->value.string=(wchar_t*)HeapAlloc(GetProcessHeap(),0,(length+1)*sizeof(wchar_t));
    if(!p->value.string){HeapFree(GetProcessHeap(),0,p);return nullptr;}
    p->value.string[0]=(wchar_t)length;
    memcpy(p->value.string+1,text,length*sizeof(wchar_t));
    p->type=0x4002; // xltypeStr | xlbitDLLFree
    return p;
}
extern "C" __declspec(dllexport) void __stdcall xlAutoFree12(Oper* value){
    if(!value)return;
    HeapFree(GetProcessHeap(),0,value->value.string);
    HeapFree(GetProcessHeap(),0,value);
}
extern "C" __declspec(dllexport) int __stdcall xlAutoOpen(){
    excel=(Callback)GetProcAddress(GetModuleHandleW(nullptr),"MdCallBack12");
    if(!excel)return 0;
    wchar_t path[32768];DWORD n=GetModuleFileNameW(module,path,32768);if(!n||n>=32768)return 0;
    auto file=counted(std::wstring(path,n));auto proc=counted(L"NativeOwned");
    auto signature=counted(L"Q");auto name=counted(L"X0.NativeOwned");
    Oper args[]={textOper(file),textOper(proc),textOper(signature),textOper(name)};
    Oper* pointers[]={&args[0],&args[1],&args[2],&args[3]};
    int rc=excel(149,4,pointers,&registration);
    bool registered=rc==0&&(registration.type&0xfff)==1;
    Oper retained=registration;
#ifdef DNA_X5_RTD
    rtdLog(L"NativeRegisterCode",rc);rtdLog(L"NativeRegisterType",registration.type);
#endif
    freeResult(registration);
    registration=retained;registration.type&=0xfff;
#ifdef DNA_X5_RTD
    if(!registered)return 0;
    return rtdOpen(file);
#else
    return registered;
#endif
}
extern "C" __declspec(dllexport) int __stdcall xlAutoClose(){
    if(!excel)return 1;
#ifdef DNA_X5_RTD
    rtdClose();
#endif
    Oper result;Oper* id[]={&registration};excel(201,1,id,&result);freeResult(result);
    auto name=counted(L"X0.NativeOwned");Oper text=textOper(name);Oper* args[]={&text};
    excel(88,1,args,&result);freeResult(result);
    return 1;
}
BOOL WINAPI DllMain(HINSTANCE instance,DWORD reason,LPVOID){
    if(reason==DLL_PROCESS_ATTACH){module=instance;DisableThreadLibraryCalls(instance);}return TRUE;
}

#ifdef DNA_X5_RTD
#include "../../calc-com-host/src/rtd-interfaces.h"
#include <atomic>
#include <thread>
#include <set>
#include <vector>
#include <mutex>
#include <objbase.h>
using namespace dna_rtd;
static CLSID rtdClass;
static std::wstring rtdProgid;
static std::vector<std::wstring> privateKeys;
static Oper rtdRegistration;
static std::vector<std::pair<std::wstring,Oper>> permissionRegistrations;
static std::atomic<LONG> liveServers{0};
static std::atomic<LONG> liveFactories{0},serverLocks{0};
static std::mutex rtdLogLock;
static bool rtdArrays=false;
static HRESULT rtdValue(double start,VARIANT* result) {
    VariantInit(result);if(!rtdArrays){result->vt=VT_R8;result->dblVal=start;return S_OK;}
    SAFEARRAYBOUND bounds[]={{2,0},{2,0}};auto array=SafeArrayCreate(VT_VARIANT,2,bounds);if(!array)return E_OUTOFMEMORY;
    for(LONG row=0;row<2;++row)for(LONG col=0;col<2;++col){LONG index[]={row,col};VARIANT value{};value.vt=VT_R8;value.dblVal=start+row*2+col;auto hr=SafeArrayPutElement(array,index,&value);if(FAILED(hr)){SafeArrayDestroy(array);return hr;}}
    result->vt=VT_ARRAY|VT_VARIANT;result->parray=array;return S_OK;
}

static void rtdLogText(const wchar_t* event,const std::wstring& value){
    std::lock_guard<std::mutex> lock(rtdLogLock);
    wchar_t path[32768];auto n=GetModuleFileNameW(module,path,32768);if(!n||n>=32768)return;
    std::wstring file(path,n);file.resize(file.find_last_of(L"\\/")+1);file+=L"rtd.tsv";
    auto line=std::wstring(event)+L"\t"+std::to_wstring(GetCurrentThreadId())+L"\t"+value+L"\n";
    auto bytes=WideCharToMultiByte(CP_UTF8,0,line.data(),(int)line.size(),nullptr,0,nullptr,nullptr);
    std::string text(bytes,'\0');WideCharToMultiByte(CP_UTF8,0,line.data(),(int)line.size(),text.data(),bytes,nullptr,nullptr);
    HANDLE h=CreateFileW(file.c_str(),FILE_APPEND_DATA,FILE_SHARE_READ|FILE_SHARE_WRITE,nullptr,OPEN_ALWAYS,FILE_ATTRIBUTE_NORMAL,nullptr);
    if(h!=INVALID_HANDLE_VALUE){DWORD written;WriteFile(h,text.data(),(DWORD)text.size(),&written,nullptr);CloseHandle(h);}
}
static void rtdLog(const wchar_t* event,LONG value){rtdLogText(event,std::to_wstring(value));}
struct NumericServer final:Rtd {
    std::atomic<ULONG> refs{1};std::atomic<LONG> value{1};std::atomic<bool> stopped{false};
    Update* update=nullptr;std::set<LONG> topics;std::thread worker;bool terminated=false;
    NumericServer(){++liveServers;}
    ~NumericServer(){ServerTerminate();--liveServers;}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void** out) override{if(!out)return E_POINTER;*out=nullptr;if(iid==IID_IUnknown||iid==IID_IDispatch||iid==server_iid){*out=static_cast<Rtd*>(this);AddRef();return S_OK;}return E_NOINTERFACE;}
    ULONG STDMETHODCALLTYPE AddRef() override{return ++refs;}
    ULONG STDMETHODCALLTYPE Release() override{auto n=--refs;if(!n)delete this;return n;}
    HRESULT STDMETHODCALLTYPE GetTypeInfoCount(UINT* n) override{if(!n)return E_POINTER;*n=1;rtdLog(L"GetTypeInfoCount",1);return S_OK;}
    HRESULT STDMETHODCALLTYPE GetTypeInfo(UINT index,LCID,ITypeInfo** out) override{
        if(!out)return E_POINTER;*out=nullptr;if(index)return DISP_E_BADINDEX;ITypeLib* lib=nullptr;HRESULT hr=E_FAIL;
        HMODULE excelModule=GetModuleHandleW(L"EXCEL.EXE");wchar_t path[32768];
        if(excelModule&&GetModuleFileNameW(excelModule,path,32768))hr=LoadTypeLibEx(path,REGKIND_NONE,&lib);
        else {const GUID excelLib={0x00020813,0,0,{0xc0,0,0,0,0,0,0,0x46}};hr=LoadRegTypeLib(excelLib,1,9,0,&lib);}
        if(SUCCEEDED(hr)){hr=lib->GetTypeInfoOfGuid(server_iid,out);lib->Release();}rtdLog(L"GetTypeInfo",hr);return hr;
    }
    HRESULT STDMETHODCALLTYPE GetIDsOfNames(REFIID,LPOLESTR* names,UINT count,LCID,DISPID* ids) override{
        const wchar_t* members[]={L"ServerStart",L"ConnectData",L"RefreshData",L"DisconnectData",L"Heartbeat",L"ServerTerminate"};
        if(!names||!ids)return E_POINTER;for(UINT i=0;i<count;++i){ids[i]=DISPID_UNKNOWN;for(int j=0;j<6;++j)if(!_wcsicmp(names[i],members[j]))ids[i]=10+j;if(ids[i]==DISPID_UNKNOWN)return DISP_E_UNKNOWNNAME;}return S_OK;
    }
    HRESULT STDMETHODCALLTYPE Invoke(DISPID id,REFIID,LCID,WORD,DISPPARAMS* params,VARIANT* result,EXCEPINFO*,UINT*) override{
        if(!params)return E_POINTER;rtdLog(L"Invoke",id);auto* args=params->rgvarg;
        if(id==10&&params->cArgs==1&&result){IUnknown* object=args[0].vt==VT_DISPATCH?args[0].pdispVal:args[0].vt==VT_UNKNOWN?args[0].punkVal:nullptr;if(!object)return DISP_E_TYPEMISMATCH;
            Update* callback=nullptr;auto hr=object->QueryInterface(update_iid,(void**)&callback);if(FAILED(hr))return hr;LONG started=0;hr=ServerStart(callback,&started);callback->Release();VariantInit(result);result->vt=VT_I4;result->lVal=started;return hr;}
        if(id==11&&params->cArgs==3&&result){auto& fresh=args[0];auto& strings=args[1];auto& topic=args[2];if(topic.vt!=VT_I4||fresh.vt!=(VT_BYREF|VT_BOOL))return DISP_E_TYPEMISMATCH;
            SAFEARRAY** array=strings.vt==(VT_BYREF|VT_ARRAY|VT_VARIANT)?strings.pparray:strings.vt==(VT_ARRAY|VT_VARIANT)?&strings.parray:nullptr;if(!array)return DISP_E_TYPEMISMATCH;return ConnectData(topic.lVal,array,fresh.pboolVal,result);}
        if(id==12&&params->cArgs==1&&result&&args[0].vt==(VT_BYREF|VT_I4)){VariantInit(result);result->vt=VT_ARRAY|VT_VARIANT;return RefreshData(args[0].plVal,&result->parray);}
        if(id==13&&params->cArgs==1&&args[0].vt==VT_I4)return DisconnectData(args[0].lVal);
        if(id==14&&params->cArgs==0&&result){VariantInit(result);result->vt=VT_I4;return Heartbeat(&result->lVal);}
        if(id==15&&params->cArgs==0)return ServerTerminate();return DISP_E_MEMBERNOTFOUND;
    }
    HRESULT STDMETHODCALLTYPE ServerStart(Update* callback,LONG* result) override{
        if(!callback||!result)return E_POINTER;if(update)return E_UNEXPECTED;
        update=callback;update->AddRef();*result=1;rtdLog(L"ServerStart",0);return S_OK;
    }
    HRESULT STDMETHODCALLTYPE ConnectData(LONG id,SAFEARRAY** strings,VARIANT_BOOL* fresh,VARIANT* result) override{
        if(!strings||!*strings||!fresh||!result||!update)return E_POINTER;
        topics.insert(id);*fresh=VARIANT_TRUE;auto initial=rtdValue(value.load(),result);if(FAILED(initial))return initial;rtdLog(L"ConnectData",id);
        if(!worker.joinable()){
            IStream* stream=nullptr;auto hr=CoMarshalInterThreadInterfaceInStream(update_iid,update,&stream);if(FAILED(hr))return hr;
            worker=std::thread([this,stream]{
                auto initialized=CoInitializeEx(nullptr,COINIT_MULTITHREADED);
                Update* proxy=nullptr;auto hr=CoGetInterfaceAndReleaseStream(stream,update_iid,(void**)&proxy);
                Sleep(250);
                if(SUCCEEDED(hr)&&!stopped.load()){value.store(42);rtdLog(L"UpdateNotify",42);proxy->UpdateNotify();}
                if(proxy)proxy->Release();if(SUCCEEDED(initialized))CoUninitialize();
            });
        }return S_OK;
    }
    HRESULT STDMETHODCALLTYPE RefreshData(LONG* count,SAFEARRAY** result) override{
        if(!count||!result)return E_POINTER;*count=(LONG)topics.size();SAFEARRAYBOUND bounds[]={{2,0},{(ULONG)*count,0}};
        *result=SafeArrayCreate(VT_VARIANT,2,bounds);if(!*result)return E_OUTOFMEMORY;LONG row=0;
        for(auto topic:topics){LONG index[]={0,row};VARIANT id{};id.vt=VT_I4;id.lVal=topic;auto hr=SafeArrayPutElement(*result,index,&id);if(FAILED(hr))return hr;
            VARIANT data{};hr=rtdValue(value.load(),&data);if(FAILED(hr))return hr;index[0]=1;hr=SafeArrayPutElement(*result,index,&data);VariantClear(&data);if(FAILED(hr))return hr;++row;}
        rtdLog(L"RefreshData",value.load());return S_OK;
    }
    HRESULT STDMETHODCALLTYPE DisconnectData(LONG id) override{topics.erase(id);rtdLog(L"DisconnectData",id);return S_OK;}
    HRESULT STDMETHODCALLTYPE Heartbeat(LONG* result) override{if(!result)return E_POINTER;*result=1;rtdLog(L"Heartbeat",0);return S_OK;}
    HRESULT STDMETHODCALLTYPE ServerTerminate() override{
        if(terminated)return S_OK;terminated=true;stopped.store(true);
        if(worker.joinable()){HANDLE handle=worker.native_handle();DWORD index=0;
            CoWaitForMultipleHandles(COWAIT_DISPATCH_CALLS|COWAIT_DISPATCH_WINDOW_MESSAGES,INFINITE,1,&handle,&index);worker.join();}
        if(update){update->Release();update=nullptr;}topics.clear();rtdLog(L"ServerTerminate",0);return S_OK;
    }
};
struct Factory final:IClassFactory{
    std::atomic<ULONG> refs{1};
    Factory(){++liveFactories;}~Factory(){--liveFactories;}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void** out) override{if(!out)return E_POINTER;*out=nullptr;if(iid==IID_IUnknown||iid==IID_IClassFactory){*out=static_cast<IClassFactory*>(this);AddRef();return S_OK;}return E_NOINTERFACE;}
    ULONG STDMETHODCALLTYPE AddRef() override{return ++refs;}
    ULONG STDMETHODCALLTYPE Release() override{auto n=--refs;if(!n)delete this;return n;}
    HRESULT STDMETHODCALLTYPE CreateInstance(IUnknown* outer,REFIID iid,void** out) override{if(outer)return CLASS_E_NOAGGREGATION;auto object=new NumericServer();auto hr=object->QueryInterface(iid,out);object->Release();return hr;}
    HRESULT STDMETHODCALLTYPE LockServer(BOOL lock) override{if(lock)++serverLocks;else --serverLocks;return S_OK;}
};
STDAPI DllGetClassObject(REFCLSID clsid,REFIID iid,void** out){if(clsid!=rtdClass)return CLASS_E_CLASSNOTAVAILABLE;auto factory=new Factory();auto hr=factory->QueryInterface(iid,out);factory->Release();return hr;}
STDAPI DllCanUnloadNow(){return liveServers.load()||liveFactories.load()||serverLocks.load()?S_FALSE:S_OK;}
// dc-6332.12: xlfRtd with the arguments the formula passes, so a number, a logical or an error topic
// reaches Excel unchanged through the C API (the typed =RTD() coerces the same arguments).
extern "C" __declspec(dllexport) Oper* __stdcall RtdCall(Oper* progid,Oper* server,Oper* topic){
    thread_local Oper returned;
    Oper* args[]={progid,server,topic};
    auto rc=excel(379,3,args,&returned);rtdLog(L"xlfRtdCall",rc);if(rc){returned.type=16;returned.value.integer=15;}return &returned;
}
extern "C" __declspec(dllexport) Oper* __stdcall RtdValue(){
    thread_local Oper returned;
    auto progid=counted(rtdProgid),server=counted(L""),topic=counted(L"numeric");Oper operands[]={textOper(progid),textOper(server),textOper(topic)};Oper* args[]={&operands[0],&operands[1],&operands[2]};
    auto rc=excel(379,3,args,&returned);rtdLog(L"xlfRtd",rc);if(rc){returned.type=16;returned.value.integer=15;}return &returned;
}
static int permission(double code){
    int fn=(int)code;Oper result{},number{},other{},flag{},caller{};
    number.type=1;number.value.number=19;other.type=1;other.value.number=23;flag.type=4;flag.value.integer=0;
    auto expression=counted(L"1+1"),status=counted(L"X5 permission");Oper text=textOper(expression),statusText=textOper(status);
    std::vector<Oper*> args;
    if(fn==4)args={&number,&other};
    else if(fn==16386)args={&number};
    else if(fn==237)args={&flag};
    else if(fn==186){number.value.number=2;args={&number};}
    else if(fn==257)args={&text};
    else if(fn==32890){flag.value.integer=1;args={&flag,&statusText};}
    else if(fn==16389){excel(89,0,nullptr,&caller);args={&caller};}
    auto rc=excel(fn,(int)args.size(),args.data(),&result);
    rtdLog(L"PermissionFunction",fn);rtdLog(L"PermissionCode",rc);
    if(fn==257 && rc==0){
        rtdLog(L"EvaluateResultType",result.type);
        if((result.type&0xfff)==1){
            unsigned long long bits;static_assert(sizeof(bits)==sizeof(result.value.number));
            memcpy(&bits,&result.value.number,sizeof(bits));wchar_t encoded[17];
            swprintf_s(encoded,L"%016llx",bits);rtdLogText(L"EvaluateResultBits",encoded);
        }
    }
    if(rc==0)freeResult(result);if(fn==16389)freeResult(caller);return rc;
}
extern "C" __declspec(dllexport) int __stdcall PermissionMain(double code){rtdLog(L"PermissionContext",1);return permission(code);}
extern "C" __declspec(dllexport) int __stdcall PermissionMacro(double code){rtdLog(L"PermissionContext",2);return permission(code);}
extern "C" __declspec(dllexport) int __stdcall PermissionPool(double code){rtdLog(L"PermissionContext",3);return permission(code);}
static bool privateKey(const std::wstring& key,const std::wstring& value){
    HKEY handle;DWORD disposition=0;auto rc=RegCreateKeyExW(HKEY_CURRENT_USER,key.c_str(),0,nullptr,0,KEY_SET_VALUE,nullptr,&handle,&disposition);
    if(rc!=ERROR_SUCCESS)return false;if(disposition!=REG_CREATED_NEW_KEY){RegCloseKey(handle);return false;}
    privateKeys.push_back(key);rc=RegSetValueExW(handle,nullptr,0,REG_SZ,(const BYTE*)value.c_str(),(DWORD)((value.size()+1)*sizeof(wchar_t)));RegCloseKey(handle);return rc==ERROR_SUCCESS;
}
static void removePrivateKeys(){for(auto it=privateKeys.rbegin();it!=privateKeys.rend();++it)RegDeleteTreeW(HKEY_CURRENT_USER,it->c_str());privateKeys.clear();}
static bool rtdOpen(const std::wstring& file){
    if(FAILED(CoCreateGuid(&rtdClass)))return false;wchar_t guid[64];StringFromGUID2(rtdClass,guid,64);
    rtdProgid=L"DnaCalc.X5."+std::to_wstring(GetCurrentProcessId());
    auto path=counted(L"");wchar_t modulePath[32768];auto n=GetModuleFileNameW(module,modulePath,32768);if(!n||n>=32768)return false;auto marker=std::wstring(modulePath,n);marker.resize(marker.find_last_of(L"\\/")+1);marker+=L"rtd-array.enable";rtdArrays=GetFileAttributesW(marker.c_str())!=INVALID_FILE_ATTRIBUTES;
    auto progidKey=L"Software\\Classes\\"+rtdProgid;auto classKey=L"Software\\Classes\\CLSID\\"+std::wstring(guid);
    if(!privateKey(progidKey,L"DNA Calc X5 controlled RTD fixture")||!privateKey(classKey,L"DNA Calc X5 controlled RTD fixture")||!privateKey(progidKey+L"\\CLSID",guid)||!privateKey(classKey+L"\\InprocServer32",std::wstring(modulePath,n))){removePrivateKeys();return false;}
    auto proc=counted(L"RtdValue"),signature=counted(L"Q#"),name=counted(L"X5.RtdValue");auto moduleFile=file;
    Oper values[]={textOper(moduleFile),textOper(proc),textOper(signature),textOper(name)};Oper* args[]={&values[0],&values[1],&values[2],&values[3]};auto rc=excel(149,4,args,&rtdRegistration);auto retained=rtdRegistration;freeResult(rtdRegistration);rtdRegistration=retained;rtdRegistration.type&=0xfff;rtdLog(L"PrivateClassRegistered",0);if(rc||rtdRegistration.type!=1){rtdRegistration={};rtdClose();return false;}
    const wchar_t* procedures[]={L"PermissionMain",L"PermissionMacro",L"PermissionPool",L"RtdCall"};const wchar_t* signatures[]={L"JB",L"JB#",L"JB$",L"QQQQ"};const wchar_t* names[]={L"X5.Permission",L"X5.MacroPermission",L"X5.PoolPermission",L"X5.RtdCall"};
    for(int i=0;i<4;++i){auto p=counted(procedures[i]),s=counted(signatures[i]),nameText=counted(names[i]);Oper row[]={textOper(moduleFile),textOper(p),textOper(s),textOper(nameText)};Oper* pointers[]={&row[0],&row[1],&row[2],&row[3]};Oper id;auto code=excel(149,4,pointers,&id);auto keep=id;freeResult(id);keep.type&=0xfff;if(code||keep.type!=1){rtdClose();return false;}permissionRegistrations.push_back({names[i],keep});}return true;
}
static void rtdClose(){Oper result;for(auto& entry:permissionRegistrations){Oper* id[]={&entry.second};excel(201,1,id,&result);freeResult(result);auto name=counted(entry.first);Oper text=textOper(name);Oper* args[]={&text};excel(88,1,args,&result);freeResult(result);}permissionRegistrations.clear();if(rtdRegistration.type==1){Oper* id[]={&rtdRegistration};excel(201,1,id,&result);freeResult(result);auto name=counted(L"X5.RtdValue");Oper text=textOper(name);Oper* args[]={&text};excel(88,1,args,&result);freeResult(result);rtdRegistration={};}removePrivateKeys();rtdLog(L"AutoCloseLiveServers",liveServers.load());}
#endif
