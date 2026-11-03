// SPDX-License-Identifier: MIT
// Excel interface facts measured with REGKIND_NONE, 16.0.20527 x64.
// Activation reads only the requested private ProgID; this client registers nothing.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <oaidl.h>
#include <objbase.h>
#include <atomic>
#include <cmath>
#include <map>
#include <memory>
#include <set>
#include <string>
#include <vector>
#include "value.h"
#include "rtd-interfaces.h"

namespace {
using namespace dna_rtd;
struct Notices {
    std::atomic<bool> alive{true},dirty{false};
    std::atomic<LONG> heartbeat{15000};
    std::atomic<ULONGLONG> notified{GetTickCount64()};
};
struct Sink final : Update {
    std::atomic<ULONG> refs{1};
    std::shared_ptr<Notices> notices;
    IUnknown* marshaler=nullptr;
    explicit Sink(std::shared_ptr<Notices> n):notices(std::move(n)) {
        CoCreateFreeThreadedMarshaler(this,&marshaler);
    }
    ~Sink(){if(marshaler)marshaler->Release();}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void** out) override {
        if(!out)return E_POINTER;*out=nullptr;
        if(iid==IID_IUnknown||iid==IID_IDispatch||iid==update_iid){*out=static_cast<Update*>(this);AddRef();return S_OK;}
        if(iid==IID_IMarshal&&marshaler)return marshaler->QueryInterface(iid,out);
        return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override{return ++refs;}
    ULONG STDMETHODCALLTYPE Release() override{auto n=--refs;if(!n)delete this;return n;}
    HRESULT STDMETHODCALLTYPE GetTypeInfoCount(UINT* n) override{if(!n)return E_POINTER;*n=0;return S_OK;}
    HRESULT STDMETHODCALLTYPE GetTypeInfo(UINT,LCID,ITypeInfo** out) override{if(out)*out=nullptr;return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE GetIDsOfNames(REFIID,LPOLESTR* names,UINT count,LCID,DISPID* ids) override {
        if(!names||!ids)return E_POINTER;
        for(UINT i=0;i<count;++i){if(!_wcsicmp(names[i],L"UpdateNotify"))ids[i]=10;else if(!_wcsicmp(names[i],L"HeartbeatInterval"))ids[i]=11;else if(!_wcsicmp(names[i],L"Disconnect"))ids[i]=12;else return DISP_E_UNKNOWNNAME;}return S_OK;
    }
    HRESULT STDMETHODCALLTYPE Invoke(DISPID id,REFIID,LCID,WORD flags,DISPPARAMS* args,VARIANT* result,EXCEPINFO*,UINT*) override {
        if(id==10)return UpdateNotify();if(id==12)return Disconnect();
        if(id==11&&(flags&DISPATCH_PROPERTYGET)){if(!result)return E_POINTER;VariantInit(result);result->vt=VT_I4;return get_HeartbeatInterval(&result->lVal);}
        if(id==11&&(flags&DISPATCH_PROPERTYPUT)){if(!args||args->cArgs!=1||args->rgvarg[0].vt!=VT_I4)return DISP_E_TYPEMISMATCH;return put_HeartbeatInterval(args->rgvarg[0].lVal);}
        return DISP_E_MEMBERNOTFOUND;
    }
    HRESULT STDMETHODCALLTYPE UpdateNotify() override{if(notices->alive.load()){
        auto now=GetTickCount64(),held=notices->notified.load();
        while(held<now&&!notices->notified.compare_exchange_weak(held,now)){}
        notices->dirty.store(true);
    }return S_OK;}
    HRESULT STDMETHODCALLTYPE get_HeartbeatInterval(LONG* n) override{if(!n)return E_POINTER;*n=notices->heartbeat.load();return S_OK;}
    HRESULT STDMETHODCALLTYPE put_HeartbeatInterval(LONG n) override{if(n<0)return E_INVALIDARG;notices->heartbeat.store(n);return S_OK;}
    HRESULT STDMETHODCALLTYPE Disconnect() override{notices->alive.store(false);return S_OK;}
};
HRESULT flatten(const VARIANT& v,Value* out){
    *out={};
    switch(v.vt){
    case VT_EMPTY:case VT_NULL:return S_OK;
    case VT_R8:out->kind=1;out->number=v.dblVal;break;
    case VT_R4:out->kind=1;out->number=v.fltVal;break;
    case VT_I4:case VT_INT:out->kind=1;out->number=v.lVal;break;
    case VT_I2:out->kind=1;out->number=v.iVal;break;
    case VT_UI4:case VT_UINT:out->kind=1;out->number=v.ulVal;break;
    case VT_UI2:out->kind=1;out->number=v.uiVal;break;
    case VT_BOOL:out->kind=3;out->number=v.boolVal!=VARIANT_FALSE;break;
    case VT_BSTR:out->kind=2;out->length=SysStringLen(v.bstrVal);out->text=SysAllocStringLen(v.bstrVal,(UINT)out->length);return out->text||!out->length?S_OK:E_OUTOFMEMORY;
    // Excel 16 returns #N/A for a VARIANT SAFEARRAY topic value; it does not spill it.
    case VT_ARRAY|VT_VARIANT:out->kind=5;out->number=2042;return S_OK;
    case VT_ERROR:out->kind=5;out->number=(unsigned long)v.scode;return S_OK;
    default:return DISP_E_TYPEMISMATCH;
    }
    return std::isfinite(out->number)?S_OK:E_INVALIDARG;
}
struct Server {
    Rtd* object=nullptr;
    Sink* sink=nullptr;
    std::shared_ptr<Notices> notices=std::make_shared<Notices>();
    std::set<LONG> topics,retired;
    ULONGLONG refresh=0,heartbeat=GetTickCount64();
    bool terminated=false;
    bool stop_attempted=false;
    HRESULT stop_error=S_OK;
    HRESULT stop(){
        if(stop_attempted)return stop_error;stop_attempted=true;notices->alive.store(false);
        HRESULT first=S_OK;
        if(object){for(auto id:topics){auto hr=object->DisconnectData(id);if(FAILED(hr)&&SUCCEEDED(first))first=hr;}
            auto hr=object->ServerTerminate();if(FAILED(hr)&&SUCCEEDED(first))first=hr;}
        if(SUCCEEDED(first)){topics.clear();terminated=true;}stop_error=first;return first;
    }
    ~Server(){stop();if(object)object->Release();if(sink)sink->Release();}
};
struct Client {
    DWORD owner=GetCurrentThreadId();
    bool closed=false;
    HRESULT close_error=S_OK;
    ULONG throttle=2000;
    std::map<std::wstring,std::unique_ptr<Server>> servers;
    std::map<LONG,Server*> topics;
    HRESULT check(){return GetCurrentThreadId()!=owner?RPC_E_WRONG_THREAD:closed?CO_E_OBJNOTCONNECTED:S_OK;}
    HRESULT server(const std::wstring& progid,Server** out){
        auto held=servers.find(progid);if(held!=servers.end()){*out=held->second.get();return S_OK;}
        if(progid.empty()||progid.find_first_of(L"\\/")!=std::wstring::npos)return E_INVALIDARG;
        // CLSIDFromProgID can create an OLE1 mapping. Read the exact mapping,
        // then parse the CLSID so an unknown ProgID has no registry side effect.
        auto key=progid+L"\\CLSID";wchar_t text[64]{};DWORD bytes=sizeof(text);
        auto rc=RegGetValueW(HKEY_CLASSES_ROOT,key.c_str(),nullptr,RRF_RT_REG_SZ,nullptr,text,&bytes);
        if(rc!=ERROR_SUCCESS)return HRESULT_FROM_WIN32(rc);
        CLSID clsid{};auto hr=CLSIDFromString(text,&clsid);if(FAILED(hr))return hr;
        auto value=std::make_unique<Server>();
        hr=CoCreateInstance(clsid,nullptr,CLSCTX_INPROC_SERVER|CLSCTX_LOCAL_SERVER,server_iid,(void**)&value->object);if(FAILED(hr))return hr;
        value->sink=new Sink(value->notices);LONG started=0;
        hr=value->object->ServerStart(value->sink,&started);
        if(FAILED(hr)||started<=0){auto failure=FAILED(hr)?hr:E_FAIL;auto cleanup=value->stop();
            if(FAILED(cleanup)){close_error=cleanup;closed=true;servers.emplace(progid,std::move(value));}return failure;}
        *out=value.get();servers.emplace(progid,std::move(value));return S_OK;
    }
    HRESULT connect(const wchar_t* progid,LONG topic,const wchar_t* const* strings,size_t count,Value* out){
        auto hr=check();if(FAILED(hr))return hr;if(topic<=0||topics.count(topic)||!count||count>253||!progid||!strings||!out)return E_INVALIDARG;
        Server* held=nullptr;hr=server(progid,&held);if(FAILED(hr))return hr;
        auto array=SafeArrayCreateVector(VT_VARIANT,0,(ULONG)count);if(!array)return E_OUTOFMEMORY;
        for(LONG i=0;i<(LONG)count;++i){VARIANT v{};v.vt=VT_BSTR;v.bstrVal=SysAllocString(strings[i]);hr=v.bstrVal?SafeArrayPutElement(array,&i,&v):E_OUTOFMEMORY;VariantClear(&v);if(FAILED(hr))break;}
        VARIANT initial{};VARIANT_BOOL newValues=VARIANT_TRUE;
        if(SUCCEEDED(hr))hr=held->object->ConnectData(topic,&array,&newValues,&initial);
        if(array)SafeArrayDestroy(array);
        if(SUCCEEDED(hr)){
            held->topics.insert(topic);topics.emplace(topic,held);
            hr=flatten(initial,out);
            if(FAILED(hr)){held->object->DisconnectData(topic);held->topics.erase(topic);held->retired.insert(topic);topics.erase(topic);}
        }
        VariantClear(&initial);return hr;
    }
    using Publish=HRESULT(WINAPI*)(void*,LONG,const Value*);
    HRESULT poll(Publish publish,void* context){
        auto hr=check();if(FAILED(hr))return hr;auto now=GetTickCount64();
        for(auto& entry:servers){auto& server=*entry.second;
            if(!server.notices->alive.load())return CO_E_OBJNOTCONNECTED;
            auto heartbeat=server.notices->heartbeat.load();
            // Excel measures this interval from UpdateNotify, not just from
            // ServerStart/the previous heartbeat. A background notice may be
            // newer than this poll's sampled clock; do not underflow the age.
            auto last=server.notices->notified.load();if(server.heartbeat>last)last=server.heartbeat;
            if(heartbeat>0&&now>=last&&now-last>=(ULONGLONG)heartbeat){LONG alive=0;hr=server.object->Heartbeat(&alive);server.heartbeat=now;if(FAILED(hr))return hr;if(alive<=0)return CO_E_OBJNOTCONNECTED;}
            if(now-server.refresh<throttle||!server.notices->dirty.exchange(false))continue;
            LONG count=0;SAFEARRAY* array=nullptr;hr=server.object->RefreshData(&count,&array);server.refresh=now;
            if(FAILED(hr)){if(array)SafeArrayDestroy(array);return hr;}
            if(!array){if(count==0)continue;return E_UNEXPECTED;}
            LONG a=0,b=-1,c=0,d=-1;VARTYPE type=VT_EMPTY;
            hr=SafeArrayGetVartype(array,&type);
            if(FAILED(hr)||type!=VT_VARIANT||SafeArrayGetDim(array)!=2||count<0||
               FAILED(SafeArrayGetLBound(array,1,&a))||FAILED(SafeArrayGetUBound(array,1,&b))||
               FAILED(SafeArrayGetLBound(array,2,&c))||FAILED(SafeArrayGetUBound(array,2,&d))||b-a+1!=2||d-c+1!=count){SafeArrayDestroy(array);return DISP_E_TYPEMISMATCH;}
            for(LONG i=0;i<count&&SUCCEEDED(hr);++i){LONG index[]={a,c+i};VARIANT id{},value{};
                hr=SafeArrayGetElement(array,index,&id);index[0]=a+1;
                if(SUCCEEDED(hr))hr=SafeArrayGetElement(array,index,&value);
                if(SUCCEEDED(hr)&&id.vt!=VT_I4)hr=DISP_E_TYPEMISMATCH;
                // A refresh queued before DisconnectData may still name its old
                // transport. It cannot publish into a later connection.
                bool active=SUCCEEDED(hr)&&server.topics.count(id.lVal);
                if(SUCCEEDED(hr)&&!active&&!server.retired.count(id.lVal))hr=E_INVALIDARG;
                Value copied{};if(active)hr=flatten(value,&copied);
                if(active&&SUCCEEDED(hr))hr=publish(context,id.lVal,&copied);
                if(copied.kind==2)SysFreeString((BSTR)copied.text);
                VariantClear(&id);VariantClear(&value);
            }
            SafeArrayDestroy(array);if(FAILED(hr))return hr;
        }return S_OK;
    }
    HRESULT disconnect(LONG topic){auto hr=check();if(FAILED(hr))return hr;auto found=topics.find(topic);if(found==topics.end())return E_INVALIDARG;
        auto* server=found->second;hr=server->object->DisconnectData(topic);if(FAILED(hr))return hr;server->topics.erase(topic);server->retired.insert(topic);topics.erase(found);
        if(server->topics.empty()){
            hr=server->stop();if(FAILED(hr)){close_error=hr;closed=true;return hr;}
            for(auto entry=servers.begin();entry!=servers.end();++entry)if(entry->second.get()==server){servers.erase(entry);break;}
        }
        return S_OK;}
    HRESULT close(){if(GetCurrentThreadId()!=owner)return RPC_E_WRONG_THREAD;if(closed)return close_error;closed=true;HRESULT first=S_OK;
        for(auto& entry:servers){auto hr=entry.second->stop();if(FAILED(hr)&&SUCCEEDED(first))first=hr;}if(SUCCEEDED(first)){topics.clear();servers.clear();}close_error=first;return first;}
    ~Client(){if(FAILED(close()))for(auto& entry:servers)entry.second.release();}
};
}
extern "C" void* dna_rtd_create(){try{return new Client();}catch(...){return nullptr;}}
extern "C" HRESULT dna_rtd_connect(void* client,const wchar_t* progid,LONG topic,const wchar_t* const* strings,size_t count,Value* out){try{return ((Client*)client)->connect(progid,topic,strings,count,out);}catch(...){return E_FAIL;}}
extern "C" HRESULT dna_rtd_poll(void* client,Client::Publish publish,void* context){try{return ((Client*)client)->poll(publish,context);}catch(...){return E_FAIL;}}
extern "C" HRESULT dna_rtd_disconnect(void* client,LONG topic){try{return ((Client*)client)->disconnect(topic);}catch(...){return E_FAIL;}}
extern "C" HRESULT dna_rtd_close(void* client){try{return ((Client*)client)->close();}catch(...){return E_FAIL;}}
extern "C" void dna_rtd_destroy(void* client){delete (Client*)client;}
extern "C" void dna_rtd_value_clear(Value* value){if(value->kind==2)SysFreeString((BSTR)value->text);*value={};}

namespace {
// Controlled server for the real client's queued-refresh regression test.
class QueuedRtd final : public Rtd {
    ULONG references=1;
public:
    bool unknown=false;
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void** out) override {if(!out)return E_POINTER;*out=nullptr;if(iid!=IID_IUnknown&&iid!=IID_IDispatch&&iid!=server_iid)return E_NOINTERFACE;*out=static_cast<Rtd*>(this);AddRef();return S_OK;}
    ULONG STDMETHODCALLTYPE AddRef() override{return ++references;}
    ULONG STDMETHODCALLTYPE Release() override{auto n=--references;if(!n)delete this;return n;}
    HRESULT STDMETHODCALLTYPE GetTypeInfoCount(UINT* out) override{if(!out)return E_POINTER;*out=0;return S_OK;}
    HRESULT STDMETHODCALLTYPE GetTypeInfo(UINT,LCID,ITypeInfo**) override{return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE GetIDsOfNames(REFIID,LPOLESTR*,UINT,LCID,DISPID*) override{return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE Invoke(DISPID,REFIID,LCID,WORD,DISPPARAMS*,VARIANT*,EXCEPINFO*,UINT*) override{return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE ServerStart(Update*,LONG* out) override{if(!out)return E_POINTER;*out=1;return S_OK;}
    HRESULT STDMETHODCALLTYPE ConnectData(LONG id,SAFEARRAY**,VARIANT_BOOL* fresh,VARIANT* out) override{if(!fresh||!out)return E_POINTER;*fresh=VARIANT_TRUE;VariantInit(out);out->vt=VT_R8;out->dblVal=id*11.;return S_OK;}
    HRESULT STDMETHODCALLTYPE RefreshData(LONG* count,SAFEARRAY** out) override {
        if(!count||!out)return E_POINTER;*count=3;SAFEARRAYBOUND bounds[]={{2,0},{3,0}};*out=SafeArrayCreate(VT_VARIANT,2,bounds);if(!*out)return E_OUTOFMEMORY;
        for(LONG row=0;row<3;++row){LONG index[]={0,row};VARIANT id{};id.vt=VT_I4;id.lVal=unknown&&row==0?4:row+1;auto hr=SafeArrayPutElement(*out,index,&id);if(FAILED(hr))return hr;
            index[0]=1;VARIANT value{};value.vt=VT_R8;value.dblVal=11.*(row+1);hr=SafeArrayPutElement(*out,index,&value);if(FAILED(hr))return hr;}
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE DisconnectData(LONG) override{return S_OK;}
    HRESULT STDMETHODCALLTYPE Heartbeat(LONG* out) override{if(!out)return E_POINTER;*out=1;return S_OK;}
    HRESULT STDMETHODCALLTYPE ServerTerminate() override{return S_OK;}
};
}
extern "C" HRESULT dna_rtd_test_obsolete_transport() noexcept {
    try {
        Client client;client.throttle=0;
        auto server=std::make_unique<Server>();server->object=new QueuedRtd();server->topics={1,2};auto held=server.get();
        client.servers.emplace(L"fixture",std::move(server));client.topics.emplace(1,held);client.topics.emplace(2,held);
        auto hr=client.disconnect(1);if(FAILED(hr))return hr;
        const wchar_t* strings[]={L"reconnect"};Value initial{};hr=client.connect(L"fixture",3,strings,1,&initial);if(FAILED(hr)||initial.number!=33)return E_FAIL;
        held->notices->dirty.store(true);
        std::vector<std::pair<LONG,double>> values;
        auto publish=+[](void* context,LONG topic,const Value* value)->HRESULT {if(value->kind!=1)return DISP_E_TYPEMISMATCH;static_cast<std::vector<std::pair<LONG,double>>*>(context)->emplace_back(topic,value->number);return S_OK;};
        hr=client.poll(publish,&values);if(FAILED(hr))return hr;
        if(values!=std::vector<std::pair<LONG,double>>{{2,22},{3,33}})return E_FAIL;
        static_cast<QueuedRtd*>(held->object)->unknown=true;held->notices->dirty.store(true);
        if(client.poll(publish,&values)!=E_INVALIDARG)return E_FAIL;
        return client.close();
    }catch(...){return E_OUTOFMEMORY;}
}
