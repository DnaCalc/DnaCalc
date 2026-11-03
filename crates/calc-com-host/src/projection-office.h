// SPDX-License-Identifier: MIT
// Included inside the projection's namespace. Office interface layouts come from
// inspect-com.ps1; extensibility signatures are ABI facts from Excel-DNA.
constexpr GUID addins_iid={0x000c0339,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID addin_iid={0x000c033a,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID bars_iid={0x000c0302,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID bar_iid={0x000c0304,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID controls_iid={0x000c0306,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID control_iid={0x000c0308,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID popup_iid={0x000c030a,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID button_iid={0x000c030e,0,0,{0xc0,0,0,0,0,0,0,0x46}};
constexpr GUID extensibility_iid={0xb65ad801,0xabaf,0x11d0,{0xbb,0x8b,0,0xa0,0xc9,0x0f,0x27,0x44}};
constexpr GUID ribbon_iid={0x000c0396,0,0,{0xc0,0,0,0,0,0,0,0x46}};
struct Extensibility : IDispatch {
    virtual HRESULT STDMETHODCALLTYPE OnConnection(IDispatch*,int,IDispatch*,SAFEARRAY**)=0;
    virtual HRESULT STDMETHODCALLTYPE OnDisconnection(int,SAFEARRAY**)=0;
    virtual HRESULT STDMETHODCALLTYPE OnAddInsUpdate(SAFEARRAY**)=0;
    virtual HRESULT STDMETHODCALLTYPE OnStartupComplete(SAFEARRAY**)=0;
    virtual HRESULT STDMETHODCALLTYPE OnBeginShutdown(SAFEARRAY**)=0;
};
struct Ribbon : IDispatch {virtual HRESULT STDMETHODCALLTYPE GetCustomUI(BSTR,BSTR*)=0;};
struct ComAddinState {
    CLSID clsid{};
    std::wstring progid,module,description,xml;
    Extensibility* extension=nullptr;
    IDispatch* public_object=nullptr;
    HMODULE lease=nullptr;
    bool connected=false,transition=false,disconnecting=false,ui_requested=false,object_setting=false;
    HRESULT failure=S_OK;
    unsigned long long generation=0;
    ~ComAddinState() {
        // A failed disconnection retains native code and the provider's object.
        // Releasing their leases would not establish that its callbacks stopped.
        // Foreign releases happen only at explicit state transitions while the
        // COMAddIn object is retained. Final destruction never calls a provider.
    }
};
bool retain_office_owner(Object* o) {
    return o->com_addin&&(o->com_addin->connected||o->com_addin->transition||FAILED(o->com_addin->failure)
        ||o->com_addin->public_object||o->com_addin->extension||o->com_addin->lease);
}
bool office_iid(Object* o,REFIID iid) {
    switch(o->office) {
    case OfficeRole::Addins:return iid==addins_iid;
    case OfficeRole::Addin:return iid==addin_iid;
    case OfficeRole::Bars:return iid==bars_iid;
    case OfficeRole::Bar:return iid==bar_iid;
    case OfficeRole::Controls:return iid==controls_iid;
    case OfficeRole::Popup:return iid==popup_iid||iid==control_iid;
    case OfficeRole::Button:return iid==button_iid||iid==control_iid;
    default:return false;
    }
}
HRESULT office_names(Object* o,LPOLESTR* text,UINT count,DISPID* ids) {
    HRESULT hr=S_OK;
    for(UINT i=0;i<count;++i) {
        auto name=text[i];auto& id=ids[i];id=DISPID_UNKNOWN;
        if(!_wcsicmp(name,L"Application"))id=0x60020000;
        else if(!_wcsicmp(name,L"Creator"))id=0x60020001;
        else if(o->office==OfficeRole::Addins) {
            if(!_wcsicmp(name,L"Item"))id=0;else if(!_wcsicmp(name,L"Count"))id=1;
            else if(!_wcsicmp(name,L"Update"))id=2;else if(!_wcsicmp(name,L"Parent"))id=3;
        } else if(o->office==OfficeRole::Addin) {
            if(!_wcsicmp(name,L"Description"))id=0;else if(!_wcsicmp(name,L"ProgId"))id=3;
            else if(!_wcsicmp(name,L"Guid"))id=4;else if(!_wcsicmp(name,L"Connect"))id=6;
            else if(!_wcsicmp(name,L"Object"))id=7;else if(!_wcsicmp(name,L"Parent"))id=8;
        } else if(o->office==OfficeRole::Bars) {
            if(!_wcsicmp(name,L"Item"))id=0;else if(!_wcsicmp(name,L"Count"))id=0x60030003;
            else if(!_wcsicmp(name,L"Parent"))id=0x6003000f;
        } else if(o->office==OfficeRole::Bar) {
            if(!_wcsicmp(name,L"Controls"))id=0x60040003;else if(!_wcsicmp(name,L"Name"))id=0x6004000e;
        } else if(o->office==OfficeRole::Controls) {
            if(!_wcsicmp(name,L"Item"))id=0;else if(!_wcsicmp(name,L"Count"))id=0x60030001;
            else if(!_wcsicmp(name,L"Add"))id=0x60030000;else if(!_wcsicmp(name,L"Parent"))id=0x60030004;
        } else {
            if(!_wcsicmp(name,L"Caption"))id=0x60040003;else if(!_wcsicmp(name,L"OnAction"))id=0x6004001a;
            else if(!_wcsicmp(name,L"Delete"))id=0x60040007;
            else if(o->office==OfficeRole::Popup&&!_wcsicmp(name,L"Controls"))id=0x60050001;
        }
        if(id==DISPID_UNKNOWN)hr=DISP_E_UNKNOWNNAME;
    }
    return hr;
}
HRESULT office_dispatch(Object* o,VARIANT* result) {
    if(result){result->vt=VT_DISPATCH;result->pdispVal=reinterpret_cast<IDispatch*>(o);add_ref(o);}return S_OK;
}
HRESULT office_string(const std::wstring& text,VARIANT* result) {
    if(result){result->vt=VT_BSTR;result->bstrVal=SysAllocStringLen(text.data(),static_cast<UINT>(text.size()));if(!result->bstrVal)return E_OUTOFMEMORY;}return S_OK;
}
Object* new_office(Object* app,Object* parent,OfficeRole role);
HRESULT connect_addin(Object* item,bool connected,bool shutdown=false) {
    auto& d=*item->com_addin;
    if(connected&&item->state->office_closing)return CO_E_OBJNOTCONNECTED;
    if(FAILED(d.failure))return d.failure;
    if(d.transition)return E_UNEXPECTED;
    if(d.connected==connected)return S_OK;
    if(d.generation==~0ull)return E_UNEXPECTED;
    ++d.generation;
    struct Transition {ComAddinState& data;Transition(ComAddinState& d,bool connect):data(d){data.transition=true;data.disconnecting=!connect;}~Transition(){data.transition=false;data.disconnecting=false;}} transition(d,connected);
    SAFEARRAY* custom=nullptr;
    if(!connected) {
        HRESULT hr=S_OK;
        if(shutdown)hr=d.extension->OnBeginShutdown(&custom);
        if(custom){SafeArrayDestroy(custom);custom=nullptr;}
        HRESULT disconnected=d.extension->OnDisconnection(shutdown?0:1,&custom);
        if(custom)SafeArrayDestroy(custom);
        if(FAILED(hr)||FAILED(disconnected)){d.failure=FAILED(hr)?hr:disconnected;return d.failure;}
        d.connected=false;d.xml.clear();d.ui_requested=false;
        auto old_object=d.public_object;d.public_object=nullptr;
        auto old_extension=d.extension;d.extension=nullptr;
        auto old_lease=d.lease;d.lease=nullptr;
        if(old_object)old_object->Release();
        old_extension->Release();
        if(old_lease)FreeLibrary(old_lease);
        return S_OK;
    }
    auto& state=*item->state;
    if(!state.owns_module||!state.owns_module(state.context,d.module.data(),d.module.size()))return E_ACCESSDENIED;
    d.lease=LoadLibraryExW(d.module.c_str(),nullptr,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
    if(!d.lease)return HRESULT_FROM_WIN32(GetLastError());
    HRESULT hr=CoCreateInstance(d.clsid,nullptr,CLSCTX_INPROC_SERVER,extensibility_iid,reinterpret_cast<void**>(&d.extension));
    if(FAILED(hr)){FreeLibrary(d.lease);d.lease=nullptr;return hr;}
    SAFEARRAYBOUND bound{1,1};custom=SafeArrayCreate(VT_VARIANT,1,&bound);
    if(!custom){d.extension->Release();d.extension=nullptr;FreeLibrary(d.lease);d.lease=nullptr;return E_OUTOFMEMORY;}
    VARIANT startup{};startup.vt=VT_I4;startup.lVal=3;LONG index=1;
    hr=SafeArrayPutElement(custom,&index,&startup);
    if(FAILED(hr)){SafeArrayDestroy(custom);d.extension->Release();d.extension=nullptr;FreeLibrary(d.lease);d.lease=nullptr;return hr;}
    d.connected=true;
    if(SUCCEEDED(hr))hr=d.extension->OnConnection(reinterpret_cast<IDispatch*>(item->application),0,reinterpret_cast<IDispatch*>(item),&custom);
    if(custom){SafeArrayDestroy(custom);custom=nullptr;}
    if(SUCCEEDED(hr))hr=d.extension->OnAddInsUpdate(&custom);
    if(custom)SafeArrayDestroy(custom);
    if(FAILED(hr)) {d.failure=hr;return hr;}
    return S_OK;
}
bool registry_string(const std::wstring& key,const wchar_t* name,std::wstring& out) {
    DWORD bytes=0,type=0;
    auto hr=RegGetValueW(HKEY_CURRENT_USER,key.c_str(),name,RRF_RT_REG_SZ,&type,nullptr,&bytes);
    if(hr!=ERROR_SUCCESS||bytes<sizeof(wchar_t)||bytes>65536)return false;
    std::vector<wchar_t> value(bytes/sizeof(wchar_t));
    if(RegGetValueW(HKEY_CURRENT_USER,key.c_str(),name,RRF_RT_REG_SZ,&type,value.data(),&bytes)!=ERROR_SUCCESS)return false;
    if(bytes%sizeof(wchar_t))return false;
    value.resize(bytes/sizeof(wchar_t));
    auto end=std::find(value.begin(),value.end(),L'\0');
    if(end==value.end()||!std::all_of(end,value.end(),[](wchar_t c){return c==0;}))return false;
    out.assign(value.begin(),end);return true;
}
HRESULT refresh_addins(Object* collection) {
    if(collection->state->office_closing)return CO_E_OBJNOTCONNECTED;
    HKEY key=nullptr;
    auto status=RegOpenKeyExW(HKEY_CURRENT_USER,L"Software\\Microsoft\\Office\\Excel\\Addins",0,KEY_READ,&key);
    if(status==ERROR_FILE_NOT_FOUND)return S_OK;
    if(status!=ERROR_SUCCESS)return HRESULT_FROM_WIN32(status);
    struct Key {HKEY value;~Key(){RegCloseKey(value);}} close{key};
    for(DWORD i=0;;++i) {
        wchar_t name[256];DWORD length=256;status=RegEnumKeyExW(key,i,name,&length,nullptr,nullptr,nullptr,nullptr);
        if(status==ERROR_NO_MORE_ITEMS)break;
        if(status!=ERROR_SUCCESS)return HRESULT_FROM_WIN32(status);
        std::wstring progid(name,length);
        if(std::any_of(collection->children.begin(),collection->children.end(),[&](Object* o){return !_wcsicmp(o->com_addin->progid.c_str(),progid.c_str());}))continue;
        std::wstring clsid,module,description;
        if(!registry_string(L"Software\\Classes\\"+progid+L"\\CLSID",nullptr,clsid))continue;
        CLSID id{};if(FAILED(CLSIDFromString(clsid.c_str(),&id)))continue;
        if(!registry_string(L"Software\\Classes\\CLSID\\"+clsid+L"\\InprocServer32",nullptr,module))continue;
        auto& state=*collection->state;
        // Discovery is read-only and restricted to modules loaded by this host.
        // An owner's other Excel add-in registrations are never activated here.
        if(!state.owns_module||!state.owns_module(state.context,module.data(),module.size()))continue;
        registry_string(L"Software\\Microsoft\\Office\\Excel\\Addins\\"+progid,L"Description",description);
        auto item=new_office(collection->application,collection,OfficeRole::Addin);
        std::unique_ptr<Object,decltype(&release)> owned(item,release);
        item->com_addin=std::make_shared<ComAddinState>();
        auto& data=*item->com_addin;data.progid=progid;data.clsid=id;data.module=module;data.description=description;
        collection->children.push_back(item);
        owned.release();
        HRESULT hr=connect_addin(item,true);if(FAILED(hr))return hr;
    }
    return S_OK;
}
HRESULT WINAPI office_count(Object* o,LONG* value) {
    if(!value)return E_POINTER;HRESULT hr=valid(o);if(FAILED(hr))return hr;
    *value=static_cast<LONG>(std::count_if(o->children.begin(),o->children.end(),[](Object* child){return !child->deleted;}));return S_OK;
}
HRESULT office_item(Object* o,const VARIANT& index,IDispatch** value) {
    if(!value)return E_POINTER;*value=nullptr;
    HRESULT alive=valid(o);if(FAILED(alive))return alive;
    if(o->deleted)return CO_E_OBJNOTCONNECTED;
    VARIANT key{};HRESULT hr=VariantCopyInd(&key,&index);if(FAILED(hr))return hr;
    Object* found=nullptr;
    if(key.vt==VT_BSTR) {
        for(auto child:o->children)if(!child->deleted) {
            auto& text=child->com_addin?child->com_addin->progid:child->caption;
            if(!_wcsicmp(text.c_str(),key.bstrVal)){found=child;break;}
        }
    } else {
        VARIANT number{};hr=VariantChangeType(&number,&key,0,VT_I4);
        if(SUCCEEDED(hr)&&number.lVal>=1){LONG i=0;for(auto child:o->children)if(!child->deleted&&++i==number.lVal){found=child;break;}}
        VariantClear(&number);
    }
    VariantClear(&key);if(!found)return DISP_E_BADINDEX;
    add_ref(found);*value=reinterpret_cast<IDispatch*>(found);return S_OK;
}
HRESULT WINAPI early_office_item(Object* o,VARIANT* index,IDispatch** value) {if(!index)return E_POINTER;return office_item(o,*index,value);}
HRESULT WINAPI office_update(Object* o) {try{HRESULT hr=valid(o);return FAILED(hr)?hr:refresh_addins(o);}catch(const std::bad_alloc&){return E_OUTOFMEMORY;}catch(...){return E_FAIL;}}
HRESULT WINAPI office_connect_get(Object* o,VARIANT_BOOL* value) {if(!value)return E_POINTER;HRESULT hr=valid(o);if(SUCCEEDED(hr))*value=o->com_addin->connected?VARIANT_TRUE:VARIANT_FALSE;return hr;}
HRESULT WINAPI office_connect_put(Object* o,VARIANT_BOOL value) {HRESULT hr=valid(o);return FAILED(hr)?hr:connect_addin(o,value!=VARIANT_FALSE);}
std::array<void*,89> office_table(OfficeRole role) {
    std::array<void*,89> table;table.fill(reinterpret_cast<void*>(unsupported));
    for(size_t i=0;i<7;++i)table[i]=application_vtable[i];table[7]=reinterpret_cast<void*>(get_application);
    if(role==OfficeRole::Addins){table[9]=reinterpret_cast<void*>(early_office_item);table[10]=reinterpret_cast<void*>(office_count);table[12]=reinterpret_cast<void*>(office_update);table[13]=reinterpret_cast<void*>(get_parent);}
    if(role==OfficeRole::Addin){table[13]=reinterpret_cast<void*>(office_connect_get);table[14]=reinterpret_cast<void*>(office_connect_put);table[17]=reinterpret_cast<void*>(get_parent);}
    return table;
}
Object* new_office(Object* app,Object* parent,OfficeRole role) {
    static auto addins=office_table(OfficeRole::Addins),addin=office_table(OfficeRole::Addin),other=office_table(OfficeRole::None);
    auto object=new Object{role==OfficeRole::Addins?addins.data():(role==OfficeRole::Addin?addin.data():other.data()),1,app->state,app,false,parent};
    object->office=role;add_ref(app);if(parent&&parent!=app)add_ref(parent);return object;
}
Object* office_controls(Object* o) {
    if(!o->controls)o->controls=new_office(o->application,o,OfficeRole::Controls);
    return o->controls;
}
HRESULT WINAPI get_addins(Object* app,IDispatch** value) {
    if(!value)return E_POINTER;*value=nullptr;HRESULT hr=valid(app);if(FAILED(hr))return hr;
    try {
    if(!app->state->addins)app->state->addins=new_office(app,app,OfficeRole::Addins);
    hr=refresh_addins(app->state->addins);if(FAILED(hr))return hr;
    add_ref(app->state->addins);*value=reinterpret_cast<IDispatch*>(app->state->addins);return S_OK;
    }catch(const std::bad_alloc&){return E_OUTOFMEMORY;}catch(...){return E_FAIL;}
}
HRESULT WINAPI get_bars(Object* app,IDispatch** value) {
    if(!value)return E_POINTER;*value=nullptr;HRESULT hr=valid(app);if(FAILED(hr))return hr;
    try {
    if(!app->state->bars) {
        auto bars=new_office(app,app,OfficeRole::Bars);std::unique_ptr<Object,decltype(&release)> owned_bars(bars,release);
        auto worksheet=new_office(app,bars,OfficeRole::Bar);std::unique_ptr<Object,decltype(&release)> owned_sheet(worksheet,release);
        worksheet->caption=L"Worksheet Menu Bar";bars->children.push_back(worksheet);owned_sheet.release();app->state->bars=owned_bars.release();
    }
    add_ref(app->state->bars);*value=reinterpret_cast<IDispatch*>(app->state->bars);return S_OK;
    }catch(const std::bad_alloc&){return E_OUTOFMEMORY;}catch(...){return E_FAIL;}
}
HRESULT WINAPI get_automation_security(Object* app,LONG* value) {if(!value)return E_POINTER;HRESULT hr=valid(app);if(SUCCEEDED(hr))*value=1;return hr;}
HRESULT WINAPI put_automation_security(Object*,LONG) {return E_NOTIMPL;}
HRESULT office_invoke(Object* o,DISPID id,WORD flags,DISPPARAMS* params,VARIANT* result) {
    if(o->deleted)return CO_E_OBJNOTCONNECTED;
    bool get=(flags&DISPATCH_PROPERTYGET)!=0,put=(flags&(DISPATCH_PROPERTYPUT|DISPATCH_PROPERTYPUTREF))!=0,method=(flags&DISPATCH_METHOD)!=0;
    if(id==0x60020000&&get&&params->cArgs==0)return office_dispatch(o->application,result);
    if(id==0x60020001&&get&&params->cArgs==0){if(result){result->vt=VT_I4;result->lVal=0x5843454c;}return S_OK;}
    bool collection=o->office==OfficeRole::Addins||o->office==OfficeRole::Bars||o->office==OfficeRole::Controls;
    if(collection&&id==0&&(get||method)) {
        if(params->cArgs!=1)return DISP_E_BADPARAMCOUNT;IDispatch* item=nullptr;HRESULT hr=office_item(o,params->rgvarg[0],&item);
        if(SUCCEEDED(hr)){if(result){result->vt=VT_DISPATCH;result->pdispVal=item;}else item->Release();}return hr;
    }
    if(collection&&get&&(id==(o->office==OfficeRole::Addins?1:o->office==OfficeRole::Bars?0x60030003:0x60030001))) {
        if(params->cArgs)return DISP_E_BADPARAMCOUNT;LONG count;HRESULT hr=office_count(o,&count);if(SUCCEEDED(hr)&&result){result->vt=VT_I4;result->lVal=count;}return hr;
    }
    if(o->office==OfficeRole::Addins&&id==2&&method){if(params->cArgs)return DISP_E_BADPARAMCOUNT;return refresh_addins(o);}
    if(o->office==OfficeRole::Addin) {
        auto& d=*o->com_addin;
        if(id==6) {
            if(get&&params->cArgs==0){if(result){result->vt=VT_BOOL;result->boolVal=d.connected?VARIANT_TRUE:VARIANT_FALSE;}return S_OK;}
            if(put&&params->cArgs==1){VARIANT value{};HRESULT hr=VariantChangeType(&value,&params->rgvarg[0],0,VT_BOOL);if(SUCCEEDED(hr))hr=connect_addin(o,value.boolVal!=VARIANT_FALSE);VariantClear(&value);return hr;}
            return DISP_E_BADPARAMCOUNT;
        }
        if(get&&params->cArgs==0) {
            if(id==0)return office_string(d.description,result);if(id==3)return office_string(d.progid,result);
            if(id==4){wchar_t text[40];StringFromGUID2(d.clsid,text,40);return office_string(text,result);}
            if(id==7){if(result){result->vt=VT_DISPATCH;result->pdispVal=d.public_object;if(d.public_object)d.public_object->AddRef();}return S_OK;}
            if(id==8)return office_dispatch(o->parent,result);
        }
        if(put&&id==7&&params->cArgs==1) {
            auto& arg=params->rgvarg[0];if(arg.vt!=VT_DISPATCH&&arg.vt!=VT_EMPTY)return DISP_E_TYPEMISMATCH;
            auto next=arg.vt==VT_DISPATCH?arg.pdispVal:nullptr;
            if(d.object_setting||next&&(!d.connected||d.disconnecting||o->state->office_closing))return E_UNEXPECTED;
            struct Setting {bool& flag;Setting(bool& f):flag(f){flag=true;}~Setting(){flag=false;}} setting(d.object_setting);
            const auto generation=d.generation;
            if(next){next->AddRef();if(d.generation!=generation||!d.connected||d.disconnecting||o->state->office_closing){next->Release();return E_ABORT;}}auto old=d.public_object;d.public_object=next;if(old)old->Release();return S_OK;
        }
    }
    if((o->office==OfficeRole::Bar&&id==0x60040003||o->office==OfficeRole::Popup&&id==0x60050001)&&get&&params->cArgs==0)return office_dispatch(office_controls(o),result);
    if(o->office==OfficeRole::Controls&&id==0x60030000&&method) {
        if(params->cArgs<1||params->cArgs>5)return DISP_E_BADPARAMCOUNT;
        VARIANT kind{};HRESULT hr=VariantChangeType(&kind,&params->rgvarg[params->cArgs-1],0,VT_I4);if(FAILED(hr))return hr;
        auto role=kind.lVal==1?OfficeRole::Button:kind.lVal==10?OfficeRole::Popup:OfficeRole::None;VariantClear(&kind);if(role==OfficeRole::None)return E_NOTIMPL;
        auto argument=[&](UINT i)->const VARIANT& {return params->rgvarg[params->cArgs-1-i];};
        auto missing=[](const VARIANT& v){return v.vt==VT_EMPTY||v.vt==VT_ERROR&&v.scode==DISP_E_PARAMNOTFOUND;};
        if(params->cArgs>1&&!missing(argument(1))) {
            VARIANT id{};hr=VariantChangeType(&id,&argument(1),0,VT_I4);
            bool supported=SUCCEEDED(hr)&&id.lVal==1;VariantClear(&id);if(!supported)return E_NOTIMPL;
        }
        if(params->cArgs>2&&!missing(argument(2)))return E_NOTIMPL;
        size_t before=o->children.size();
        if(params->cArgs>3&&!missing(argument(3))) {
            VARIANT position{};hr=VariantChangeType(&position,&argument(3),0,VT_I4);
            if(FAILED(hr))return hr;
            LONG desired=position.lVal;VariantClear(&position);if(desired<1)return E_INVALIDARG;
            LONG index=0;before=o->children.size();
            for(size_t n=0;n<o->children.size();++n)if(!o->children[n]->deleted&&++index==desired){before=n;break;}
            if(desired>index+1)return E_INVALIDARG;
        }
        if(params->cArgs>4&&!missing(argument(4))) {
            VARIANT temporary{};hr=VariantChangeType(&temporary,&argument(4),0,VT_BOOL);
            bool supported=SUCCEEDED(hr)&&temporary.boolVal!=VARIANT_FALSE;VariantClear(&temporary);if(!supported)return E_NOTIMPL;
        }
        auto child=new_office(o->application,o,role);
        std::unique_ptr<Object,decltype(&release)> owned(child,release);
        o->children.insert(o->children.begin()+before,child);owned.release();return office_dispatch(child,result);
    }
    if(o->office==OfficeRole::Bar||o->office==OfficeRole::Button||o->office==OfficeRole::Popup) {
        auto* property=id==(o->office==OfficeRole::Bar?0x6004000e:0x60040003)?&o->caption:id==0x6004001a?&o->action:nullptr;
        if(property&&get&&params->cArgs==0)return office_string(*property,result);
        if(property&&put&&params->cArgs==1){VARIANT value{};HRESULT hr=VariantChangeType(&value,&params->rgvarg[0],0,VT_BSTR);if(SUCCEEDED(hr))property->assign(value.bstrVal,SysStringLen(value.bstrVal));VariantClear(&value);return hr;}
        if(id==0x60040007&&method){if(params->cArgs>1)return DISP_E_BADPARAMCOUNT;o->deleted=true;return S_OK;}
    }
    return DISP_E_MEMBERNOTFOUND;
}
void clear_office_tree(Object* o) {
    auto children=std::move(o->children);for(auto child:children){clear_office_tree(child);release(child);}
    if(o->controls){auto controls=o->controls;o->controls=nullptr;clear_office_tree(controls);release(controls);}
}
HRESULT close_office(Object* app) {
    app->state->office_closing=true;
    if(FAILED(app->state->office_failure))return app->state->office_failure;
    HRESULT first=S_OK;
    if(app->state->addins) {
        auto snapshot=app->state->addins->children;for(auto item:snapshot)add_ref(item);
        for(auto item:snapshot){HRESULT hr=connect_addin(item,false,true);if(FAILED(hr)&&SUCCEEDED(first))first=hr;release(item);}
        if(FAILED(first)){app->state->office_failure=first;return first;}
        clear_office_tree(app->state->addins);
    }
    if(app->state->bars)clear_office_tree(app->state->bars);
    return first;
}
HRESULT read_ribbon_ui(Object* app) {
    if(!app->state->addins)return S_OK;
    struct Snapshot {std::vector<Object*> items;Snapshot(Object* o):items(o->children){for(auto item:items)add_ref(item);}~Snapshot(){for(auto item:items)release(item);}} snapshot(app->state->addins);
    for(auto item:snapshot.items) {
        auto& d=*item->com_addin;if(!d.connected||d.ui_requested)continue;
        const auto generation=d.generation;
        HMODULE module=nullptr;
        if(!GetModuleHandleExW(0,d.module.c_str(),&module))return HRESULT_FROM_WIN32(GetLastError());
        struct ModuleLease {HMODULE value;~ModuleLease(){FreeLibrary(value);}} lease{module};
        auto extension=d.extension;extension->AddRef();
        struct ExtensionLease {Extensibility* value;~ExtensionLease(){value->Release();}} instance{extension};
        if(d.generation!=generation||!d.connected)return E_ABORT;
        Ribbon* ribbon=nullptr;HRESULT hr=extension->QueryInterface(ribbon_iid,reinterpret_cast<void**>(&ribbon));
        if(d.generation!=generation||!d.connected){if(ribbon)ribbon->Release();return E_ABORT;}
        if(hr==E_NOINTERFACE){d.ui_requested=true;continue;}
        if(FAILED(hr))return hr;
        BSTR id=SysAllocString(L"Microsoft.Excel.Workbook"),xml=nullptr;
        if(!id){ribbon->Release();return E_OUTOFMEMORY;}
        hr=ribbon->GetCustomUI(id,&xml);SysFreeString(id);ribbon->Release();
        struct TextOwner {BSTR value;~TextOwner(){SysFreeString(value);}} text{xml};
        if(SUCCEEDED(hr)&&(d.generation!=generation||!d.connected))return E_ABORT;
        if(SUCCEEDED(hr)){d.xml.assign(xml?xml:L"",SysStringLen(xml));d.ui_requested=true;}
        if(FAILED(hr))return hr;
    }
    return S_OK;
}
