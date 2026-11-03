// SPDX-License-Identifier: MIT
// Black-box x64 boundary observation.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <iomanip>
#include <map>
#include <set>
#include <sstream>
#include <string>
#include <vector>

using Address = uint64_t;
static HANDLE process;
static DWORD processId;
static std::ofstream output;
static uint64_t sequence = 0, nextCall = 0;
static LARGE_INTEGER frequency;
static bool healthy = true;
using NtSuspendThreadFn=LONG(NTAPI*)(HANDLE,ULONG*);
static NtSuspendThreadFn ntSuspendThread;

static std::string utf8(const std::wstring& s) {
    int n = WideCharToMultiByte(CP_UTF8, 0, s.data(), (int)s.size(), nullptr, 0, nullptr, nullptr);
    std::string r(n, 0);
    WideCharToMultiByte(CP_UTF8, 0, s.data(), (int)s.size(), r.data(), n, nullptr, nullptr);
    return r;
}
static std::string quote(const std::string& s) {
    std::ostringstream r; r << '"';
    for (unsigned char c : s) {
        if (c == '"' || c == '\\') r << '\\' << c;
        else if (c < 32) r << "\\u" << std::hex << std::setw(4) << std::setfill('0') << (int)c << std::dec;
        else r << c;
    }
    r << '"'; return r.str();
}
static std::string hex(Address p) { std::ostringstream s; s << "0x" << std::hex << p; return quote(s.str()); }
static long long ticks() { LARGE_INTEGER n; QueryPerformanceCounter(&n); return n.QuadPart; }
static void emit(const std::string& fields) {
    output << "{\"seq\":" << ++sequence << ",\"qpc\":" << ticks() << "," << fields << "}\n";
    output.flush();
}
static void debuggerError(const char* operation) {
    DWORD error=GetLastError();healthy=false;
    emit("\"event\":\"debugger_error\",\"operation\":"+quote(operation)+",\"win32_error\":"+std::to_string(error));
}
static bool readBytes(Address p, void* data, size_t length) {
    if(!length)return true;
    SIZE_T done = 0;
    return ReadProcessMemory(process, (void*)p, data, length, &done) && done == length;
}
static void readError(Address p,size_t length) {
    healthy=false;emit("\"event\":\"read_error\",\"address\":"+hex(p)+",\"bytes\":"+std::to_string(length));
}
template<class T> static T read(Address p) {
    T t{};if(!readBytes(p,&t,sizeof(t)))readError(p,sizeof(t));return t;
}
static std::string ansi(Address p, size_t max = 2048) {
    std::string s;
    for (size_t i = 0; i < max; ++i) { char c; if (!readBytes(p+i, &c, 1)){readError(p+i,1);break;}if(!c)break;s+=c; }
    return s;
}
static std::wstring wide(Address p, size_t count) {
    if (count > 32768) count = 32768;
    std::wstring r(count, 0);
    if (!readBytes(p, r.data(), count*2)){readError(p,count*2);return L"<unreadable>";}
    return r;
}
static std::wstring wideZ(Address p) {
    std::wstring s;
    for (size_t i = 0; i < 32768; ++i) { wchar_t c; if (!readBytes(p+i*2, &c, 2)){readError(p+i*2,2);break;}if(!c)break;s+=c; }
    return s;
}
static std::string number(double d) {
    if (!_finite(d)) return quote(_isnan(d) ? "NaN" : (d < 0 ? "-Infinity" : "Infinity"));
    std::ostringstream s; s << std::setprecision(17) << d; return s.str();
}
static std::string rectangle(Address p) {
    int32_t r[4]{};
    if (!readBytes(p, r, sizeof(r))){readError(p,sizeof(r));return "{\"unreadable\":true}";}
    return "["+std::to_string(r[0])+","+std::to_string(r[1])+","+std::to_string(r[2])+","+std::to_string(r[3])+"]";
}
static std::string operString(Address p) {
    if ((read<uint32_t>(p+24)&0xfff) != 2) return "";
    Address s = read<Address>(p);
    if(!s){readError(s,2);return "<unreadable>";}
    return utf8(wide(s+2, read<uint16_t>(s)));
}
static std::string oper(Address p, int depth = 0) {
    if (!p) return "null";
    unsigned char bytes[32];
    if (!readBytes(p, bytes, sizeof(bytes))){readError(p,sizeof(bytes));return "{\"unreadable\":"+hex(p)+"}";}
    uint32_t type; memcpy(&type, bytes+24, 4);
    std::string s = "{\"type\":"+std::to_string(type)+",\"value\":";
    switch (type & 0xfff) {
    case 1: s += number(read<double>(p)); break;
    case 2: s += quote(operString(p)); break;
    case 4: s += read<int32_t>(p) ? "true" : "false"; break;
    case 16: case 2048: s += std::to_string(read<int32_t>(p)); break;
    case 128: case 256: s += "null"; break;
    case 1024: s += "{\"count\":"+std::to_string(read<uint16_t>(p))+",\"rect\":"+rectangle(p+4)+"}"; break;
    case 8: {
        Address refs = read<Address>(p);
        if(!refs){s+="{\"sheet\":"+hex(read<Address>(p+8))+",\"null_reference_table\":true}";break;}
        uint16_t count = read<uint16_t>(refs);
        s += "{\"sheet\":"+hex(read<Address>(p+8))+",\"areas\":[";
        for (unsigned i=0; i<count; ++i) { if (i) s+=","; s += rectangle(refs+4+i*16); }
        s += "]}"; break;
    }
    case 64: {
        Address values = read<Address>(p); int32_t rows = read<int32_t>(p+8), cols = read<int32_t>(p+12);
        int64_t count = int64_t(rows)*cols;
        s += "{\"rows\":"+std::to_string(rows)+",\"columns\":"+std::to_string(cols)+",\"items\":[";
        int64_t limit = depth < 4 ? std::min<int64_t>(4096, std::max<int64_t>(0,count)) : 0;
        for (int64_t i=0; i<limit; ++i) { if (i) s+=","; s += oper(values+i*32, depth+1); }
        s += "],\"omitted\":"+std::to_string(count-limit)+"}"; break;
    }
    case 2050: s += "{\"opaque_handle\":"+hex(read<Address>(p))+",\"byte_count\":"+std::to_string(read<int64_t>(p+8))+"}"; break;
    default: {
        std::ostringstream raw; for (unsigned char c: bytes) raw << std::hex << std::setw(2) << std::setfill('0') << (int)c;
        s += "{\"raw32\":"+quote(raw.str())+"}";
    }
    }
    return s+"}";
}

struct Entry { std::string name, signature; };
struct Breakpoint { unsigned char original; bool enabled = false; std::vector<Entry> entries; size_t returnRefs=0; };
struct Module { Address base; std::wstring path; std::map<std::string, Address> exports; uint32_t size=0; };
struct Call {
    uint64_t id; DWORD thread; Entry entry; Address stack, ret, result, args;
    long long started; int fn = 0, count = 0;
    std::array<Address,9> raw{};
    std::vector<std::string> names;
    Address run_result = 0;
    uint64_t rtd_call = 0;
    std::string clsid;
};
static std::map<Address, Breakpoint> breakpoints;
static std::map<Address, Module> modules;
static std::map<DWORD, std::vector<Call>> calls;
static std::map<DWORD, std::vector<HANDLE>> suspended;
static std::map<DWORD, Address> stepping;
static std::set<DWORD> liveThreads;
static std::set<DWORD> exitedThreads;
static std::vector<HANDLE> stoppedThreads;
static std::map<Address,std::string> objects;
static std::map<Address,std::string> interfaces;
static std::map<Address,Address> objectTables;
static std::map<Address,std::string> factoryClasses;
static std::map<Address,std::string> instanceClasses;
struct RtdCandidate { uint64_t call; DWORD thread; std::string clsid,iid; };
static std::map<Address,RtdCandidate> rtdCandidates;
static std::map<std::pair<Address,int32_t>,std::string> members;
static void arm(Address p,const Entry* entry);
static void forgetObject(Address p) {
    objects.erase(p);interfaces.erase(p);objectTables.erase(p);
    factoryClasses.erase(p);instanceClasses.erase(p);rtdCandidates.erase(p);
    for(auto it=members.begin();it!=members.end();) {if(it->first.first==p)it=members.erase(it);else ++it;}
}

static std::string guid(Address p) {
    GUID g{}; if(!readBytes(p,&g,sizeof(g))){readError(p,sizeof(g));return "unreadable";}
    char s[40];snprintf(s,sizeof(s),"%08lx-%04x-%04x-%02x%02x-%02x%02x%02x%02x%02x%02x",
        g.Data1,g.Data2,g.Data3,g.Data4[0],g.Data4[1],g.Data4[2],g.Data4[3],g.Data4[4],g.Data4[5],g.Data4[6],g.Data4[7]);return s;
}
static void watchObject(Address p,const std::string& label,const std::string& kind="dispatch") {
    if(!p)return;
    Address table=read<Address>(p);if(!table)return;
    if(objects.count(p) && objectTables[p]!=table)forgetObject(p);
    if(objects.count(p) && (interfaces[p]==kind || kind=="unknown" || (kind=="dispatch" && interfaces[p]!="unknown")))return;
    objects[p]=label;interfaces[p]=kind;objectTables[p]=table;
    auto method=[&](int slot,const char* name){Entry e{std::string("COM!")+name,""};arm(read<Address>(table+8*slot),&e);};
    method(0,"QueryInterface");
    method(2,"Release");
    if(kind=="container")method(4,"FindConnectionPoint");
    else if(kind=="point"){method(5,"Advise");method(6,"Unadvise");}
    else if(kind=="classfactory"){method(3,"CreateInstance");method(4,"LockServer");}
    else if(kind!="unknown"){method(3,"GetTypeInfoCount");method(4,"GetTypeInfo");method(5,"GetIDsOfNames");method(6,"Invoke");}
    auto early=[&](int slot,const char* member,const char* result){Entry e{"COM!early."+kind+"."+member,result};arm(read<Address>(table+8*slot),&e);};
    if(kind=="application" || kind=="workbook" || kind=="workbooks" || kind=="worksheet") {
        early(7,"Application","object1"); early(9,"Parent","object1");
    }
    if(kind=="application") {
        early(16,"ActiveSheet","object1");early(18,"ActiveWorkbook","object1");early(52,"Workbooks","object1");
        early(42,"Run","run");
        early(223,"Path","bstr2");early(281,"Version","bstr2");early(300,"EnableEvents.get","bool1");early(301,"EnableEvents.put","putbool");early(351,"Hwnd","int1");
    } else if(kind=="workbook") {
        early(14,"ActiveSheet","object1");early(50,"FullName","bstr2");early(66,"Name","bstr1");early(76,"Path","bstr2");
    } else if(kind=="workbooks") {early(12,"Count","int1");early(13,"Item","object2");}
    else if(kind=="worksheet") early(18,"Name","bstr1");
    else if(kind=="rtdserver"){early(7,"ServerStart","rtdstart");early(8,"ConnectData","rtdconnect");early(9,"RefreshData","rtdrefresh");early(10,"DisconnectData","rtddisconnect");early(11,"Heartbeat","int1");early(12,"ServerTerminate","rtdterminate");}
    else if(kind=="rtdsink"){early(7,"UpdateNotify","rtdnotify");early(8,"HeartbeatInterval.get","int1");early(9,"HeartbeatInterval.put","rtdinterval");early(10,"Disconnect","rtdterminate");}
    else if(kind=="extensibility"){early(7,"OnConnection","addinconnect");early(8,"OnDisconnection","addindisconnect");early(9,"OnAddInsUpdate","addinarray");early(10,"OnStartupComplete","addinarray");early(11,"OnBeginShutdown","addinarray");}
    else if(kind=="ribbon")early(7,"GetCustomUI","bstr2");
    emit("\"event\":\"com_object\",\"object\":"+hex(p)+",\"label\":"+quote(label)+",\"interface\":"+quote(kind));
}
static std::string variantArray(Address p);
static std::string variant(Address p,int depth=0) {
    if(!p)return "null";
    uint16_t vt=read<uint16_t>(p);Address v=read<Address>(p+8);
    std::string s="{\"vt\":"+std::to_string(vt)+",\"value\":";
    if(vt&0x4000){
        if(vt==0x400c&&depth<4)s+=variant(v,depth+1);
        else if(vt==0x4009||vt==0x400d)s+="{\"byref\":"+hex(v)+",\"object\":"+hex(read<Address>(v))+"}";
        else if(vt==0x400b)s+="{\"byref\":"+hex(v)+",\"value\":"+std::string(read<int16_t>(v)?"true":"false")+"}";
        else if(vt==0x4003)s+="{\"byref\":"+hex(v)+",\"value\":"+std::to_string(read<int32_t>(v))+"}";
        else if(vt==0x600c)s+=variantArray(read<Address>(v));
        else s+="{\"byref\":"+hex(v)+"}";
    }else switch(vt){
        case 0:case 1:s+="null";break;
        case 2:s+=std::to_string(read<int16_t>(p+8));break;
        case 3:case 10:case 22:s+=std::to_string(read<int32_t>(p+8));break;
        case 4:s+=number(read<float>(p+8));break;
        case 5:case 7:s+=number(read<double>(p+8));break;
        case 8:s+=v?quote(utf8(wide(v,read<uint32_t>(v-4)/2))):"null";break;
        case 9:case 13:s+=hex(v);break;
        case 0x200c:s+=variantArray(v);break;
        case 11:s+=read<int16_t>(p+8)?"true":"false";break;
        default:s+="{\"raw_union\":"+hex(v)+"}";
    }
    return s+"}";
}
static std::string variantArray(Address p){
    if(!p)return "null";auto dims=read<uint16_t>(p);auto width=read<uint32_t>(p+4);
    std::string s="{\"dimensions\":"+std::to_string(dims)+",\"element_bytes\":"+std::to_string(width)+",\"bounds\":[";uint64_t count=1;
    if(dims<1||dims>2||width!=24)return s+"],\"unsupported\":true}";
    for(unsigned i=0;i<dims;++i){auto n=read<uint32_t>(p+24+8*i);auto lower=read<int32_t>(p+28+8*i);if(i)s+=",";s+="{\"count\":"+std::to_string(n)+",\"lower\":"+std::to_string(lower)+"}";count*=n;}
    s+="],\"values\":[";auto data=read<Address>(p+16);for(uint64_t i=0;i<count&&i<4096;++i){if(i)s+=",";s+=variant(data+24*i);}return s+"],\"truncated\":"+(count>4096?"true":"false")+"}";
}

static bool writeByte(Address p, unsigned char byte) {
    DWORD old; SIZE_T done = 0;
    if (!VirtualProtectEx(process, (void*)p, 1, PAGE_EXECUTE_READWRITE, &old)) return false;
    bool ok = WriteProcessMemory(process, (void*)p, &byte, 1, &done) && done == 1;
    DWORD unused; VirtualProtectEx(process, (void*)p, 1, old, &unused);
    FlushInstructionCache(process, (void*)p, 1);
    return ok;
}
static void arm(Address p, const Entry* entry = nullptr) {
    if (!p) return;
    auto it = breakpoints.find(p);
    if (it == breakpoints.end()) {
        unsigned char c;
        if (!readBytes(p, &c, 1) || c == 0xcc) { healthy=false; emit("\"event\":\"breakpoint_error\",\"address\":"+hex(p)); return; }
        it = breakpoints.emplace(p, Breakpoint{c}).first;
    } else if(!it->second.enabled && it->second.entries.empty() && !it->second.returnRefs) {
        // A fresh watch can reuse dynamic code at an old, inactive return site.
        unsigned char c;
        if(!readBytes(p,&c,1) || c==0xcc){healthy=false;emit("\"event\":\"breakpoint_error\",\"address\":"+hex(p));return;}
        it->second.original=c;
    }
    if (entry) {
        auto& v=it->second.entries;
        if (std::none_of(v.begin(), v.end(), [&](const Entry& e) {return e.name==entry->name;})) v.push_back(*entry);
    }
    if (!it->second.enabled) {
        it->second.enabled = writeByte(p, 0xcc);
        if (!it->second.enabled) { healthy=false; emit("\"event\":\"breakpoint_error\",\"address\":"+hex(p)); }
    }
}
static std::string basename(const std::wstring& path) { return utf8(path.substr(path.find_last_of(L"\\/")+1)); }
static std::string location(Address p) {
    auto it = modules.upper_bound(p);
    if (it == modules.begin()) return hex(p);
    --it;
    if(p-it->first>=it->second.size)return "{\"unmapped\":"+hex(p)+"}";
    return "{\"module\":"+quote(basename(it->second.path))+",\"rva\":"+std::to_string(p-it->first)+"}";
}
static bool fromModule(Address p,const char* name) {
    auto it=modules.upper_bound(p);if(it==modules.begin())return false;--it;
    return p-it->first<it->second.size && _stricmp(basename(it->second.path).c_str(),name)==0;
}
static Module inspectModule(Address base, std::wstring path) {
    Module m{base, path};
    IMAGE_DOS_HEADER dos = read<IMAGE_DOS_HEADER>(base);
    if (dos.e_magic != IMAGE_DOS_SIGNATURE) return m;
    IMAGE_NT_HEADERS64 nt = read<IMAGE_NT_HEADERS64>(base+dos.e_lfanew);
    if (nt.Signature != IMAGE_NT_SIGNATURE || nt.OptionalHeader.Magic != IMAGE_NT_OPTIONAL_HDR64_MAGIC) return m;
    m.size=nt.OptionalHeader.SizeOfImage;
    auto dir = nt.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT];
    if (!dir.VirtualAddress) return m;
    auto ex = read<IMAGE_EXPORT_DIRECTORY>(base+dir.VirtualAddress);
    if (ex.NumberOfNames > 100000) return m;
    std::vector<DWORD> names(ex.NumberOfNames), functions(ex.NumberOfFunctions);
    std::vector<WORD> ordinals(ex.NumberOfNames);
    if (!readBytes(base+ex.AddressOfNames, names.data(), names.size()*4) ||
        !readBytes(base+ex.AddressOfNameOrdinals, ordinals.data(), ordinals.size()*2) ||
        !readBytes(base+ex.AddressOfFunctions, functions.data(), functions.size()*4)) return m;
    for (size_t i=0; i<names.size(); ++i) {
        if (ordinals[i] >= functions.size()) continue;
        DWORD rva = functions[ordinals[i]];
        if (rva >= dir.VirtualAddress && rva < dir.VirtualAddress+dir.Size) continue; // forwarded export
        m.exports.emplace(ansi(base+names[i]), base+rva);
    }
    return m;
}
static void addModule(Address base, const std::wstring& path) {
    if (modules.count(base)) return;
    Module m = inspectModule(base, path);
    std::string file=basename(path), lower=file;
    std::transform(lower.begin(),lower.end(),lower.begin(),[](unsigned char c){return (char)tolower(c);});
    bool xll = lower.size()>=4 && lower.substr(lower.size()-4)==".xll";
    if (xll || lower=="xloil.dll" || lower.rfind("xloil_",0)==0 || lower=="excel.exe" || lower=="xlcall32.dll" || lower=="oleacc.dll")
        emit("\"event\":\"module\",\"name\":"+quote(file)+",\"base\":"+hex(base)+",\"path\":"+quote(utf8(path)));
    if(xll) {
        size_t stubs=0;std::string names="[";bool first=true;
        for(auto& [name,addr]:m.exports) {
            if(name.size()>1&&name[0]=='f'&&std::all_of(name.begin()+1,name.end(),[](char c){return c>='0'&&c<='9';})){++stubs;continue;}
            if(!first)names+=",";first=false;names+=quote(name);
        }
        emit("\"event\":\"xll_exports\",\"module\":"+quote(file)+",\"numbered_stub_count\":"+std::to_string(stubs)+",\"names\":"+names+"]");
    }
    for (auto& [name,addr]:m.exports) {
        bool watch = name=="MdCallBack12" || name=="xlAutoFree12" || (lower=="xlcall32.dll" && (name=="XLCallVer" || name=="LPenHelper")) ||
            ((lower=="ole32.dll" || lower=="combase.dll") && (name=="CoRegisterClassObject" || name=="CoCreateInstance")) ||
            (lower=="oleacc.dll" && name=="AccessibleObjectFromWindow") || (lower=="xloil.dll"&&name=="DllGetClassObject") ||
            (xll && (name=="xlAutoOpen" || name=="xlAutoClose" || name=="xlAutoRemove" || name=="xlAutoAdd" ||
                name=="xlAutoFree12" || name=="xlAutoRegister12" || name=="xlAddInManagerInfo12" ||
                name=="DllGetClassObject" || name=="DllCanUnloadNow" || name=="SyncMacro" ||
                name=="CalculationCanceled" || name=="CalculationEnded"));
        if (watch) { Entry e{file+"!"+name, ""}; arm(addr,&e); }
    }
    modules.emplace(base,std::move(m));
}
static void registerEntry(const Call& c) {
    if (c.fn != 149 || c.count < 4) return;
    std::string module = operString(read<Address>(c.args));
    std::string procedure = operString(read<Address>(c.args+8));
    std::string signature = operString(read<Address>(c.args+16));
    std::string name = operString(read<Address>(c.args+24));
    auto slash=module.find_last_of("\\/"); if(slash!=std::string::npos) module=module.substr(slash+1);
    // GetModuleHandle accepts a DLL name without its extension. xlOil's native
    // plugins register exactly that form; use the same module identity here.
    if(module.find('.')==std::string::npos)module+=".dll";
    for (auto& [base,m]:modules) {
        if (_stricmp(basename(m.path).c_str(),module.c_str()) != 0) continue;
        // xlOil changes its PE export entry between registrations. Read the current table,
        // and leave the previously armed address intact for its earlier registration.
        auto current=inspectModule(base,m.path);
        auto found=current.exports.find(procedure);
        if(found==current.exports.end()) { emit("\"event\":\"unresolved_registration\",\"procedure\":"+quote(procedure)); healthy=false; return; }
        Entry e{module+"!"+procedure+":"+name,signature}; arm(found->second,&e);
        emit("\"event\":\"registered_entry\",\"call\":"+std::to_string(c.id)+",\"entry\":"+quote(e.name)+",\"signature\":"+quote(signature));
        return;
    }
}
static std::vector<std::string> signatureParts(const std::string& s) {
    std::vector<std::string> p;
    for (size_t i=0;i<s.size();++i) {
        char c=s[i]; if(c=='!'||c=='#'||c=='$'||c=='&') continue;
        std::string t(1,c); if(i+1<s.size()&&s[i+1]=='%') t+=s[++i]; p.push_back(t);
    }
    return p;
}
static Address argument(const CONTEXT& c,int i) {
    if(i==0) return c.Rcx; if(i==1) return c.Rdx; if(i==2) return c.R8; if(i==3) return c.R9;
    return read<Address>(c.Rsp+40+(i-4)*8);
}
static double doubleArgument(const CONTEXT& c,int i) {
    double d;
    if(i<4) { const M128A* xmm=&c.Xmm0; memcpy(&d,&xmm[i],8); }
    else { Address v=argument(c,i); memcpy(&d,&v,8); }
    return d;
}
static std::string typed(const std::string& t, Address v) {
    if(t=="Q"||t=="U"||t=="X") return oper(v);
    if(t=="E") return number(read<double>(v));
    if(t=="L"||t=="M") return std::to_string(read<int16_t>(v));
    if(t=="N") return std::to_string(read<int32_t>(v));
    if(t=="D%") return quote(utf8(wide(v+2,read<uint16_t>(v))));
    if(t=="K%") {
        int32_t rows=read<int32_t>(v),cols=read<int32_t>(v+4);
        int64_t count=int64_t(rows)*cols,limit=std::min<int64_t>(4096,std::max<int64_t>(0,count));
        std::string s="{\"rows\":"+std::to_string(rows)+",\"columns\":"+std::to_string(cols)+",\"items\":[";
        for(int64_t i=0;i<limit;++i){if(i)s+=",";s+=number(read<double>(v+8+i*8));}
        return s+"],\"omitted\":"+std::to_string(count-limit)+"}";
    }
    if(t=="C%") return quote(utf8(wideZ(v)));
    if(t=="J")return std::to_string((int32_t)v);
    if(t=="A"||t=="I")return std::to_string((int16_t)v);
    if(t=="H")return std::to_string((uint16_t)v);
    return "{\"pointer\":"+hex(v)+",\"decoding\":\"unsupported signature\"}";
}
static std::string entryArgs(const Entry& e,const CONTEXT& ctx) {
    auto parts=signatureParts(e.signature); std::string s="[";
    for(size_t i=1;i<parts.size();++i) {
        if(i>1)s+=",";
        s+="{\"code\":"+quote(parts[i])+",\"value\":";
        s+=parts[i]=="B" ? number(doubleArgument(ctx,(int)i-1)) : typed(parts[i],argument(ctx,(int)i-1)); s+="}";
    }
    if(parts.empty() && (e.name.find("xlAutoFree12")!=std::string::npos || e.name.find("xlAutoRegister12")!=std::string::npos || e.name.find("xlAddInManagerInfo12")!=std::string::npos)) s+=oper(ctx.Rcx);
    return s+"]";
}
static void enter(const Entry& entry, DWORD thread, const CONTEXT& ctx) {
    Call c{++nextCall,thread,entry,ctx.Rsp,read<Address>(ctx.Rsp),ctx.R9,ctx.R8,ticks()};
    for(int i=0;i<9;++i)c.raw[i]=argument(ctx,i);
    std::string fields="\"event\":\"enter\",\"call\":"+std::to_string(c.id)+",\"thread\":"+std::to_string(thread)+",\"entry\":"+quote(entry.name)+",\"from\":"+location(c.ret);
    if(entry.name.find("!CoCreateInstance")!=std::string::npos) {
        // The RTD client requests this interface explicitly. Other activations
        // are outside this observation, including unrelated Office classes.
        if(guid(ctx.R9)!="ec0e6191-db51-11d3-8f3e-00c04f3651b8")return;
        fields+=",\"clsid\":"+quote(guid(ctx.Rcx))+",\"iid\":"+quote(guid(ctx.R9));
    }else if(entry.name.find("!CoRegisterClassObject")!=std::string::npos) {
        fields+=",\"clsid\":"+quote(guid(ctx.Rcx))+",\"class_context\":"+std::to_string((uint32_t)ctx.R8)+",\"flags\":"+std::to_string((uint32_t)ctx.R9);
        watchObject(ctx.Rdx,"registered COM class factory","classfactory");
        factoryClasses[ctx.Rdx]=guid(ctx.Rcx);
    }else if(entry.name.rfind("COM!",0)==0) {
        if(!objects.count(ctx.Rcx))return; // Shared vtable code: observe only discovered objects.
        if(read<Address>(ctx.Rcx)!=objectTables[ctx.Rcx])watchObject(ctx.Rcx,"reused COM pointer","dispatch");
        if(entry.name=="COM!Invoke" && rtdCandidates.count(ctx.Rcx) && members[{ctx.Rcx,(int32_t)ctx.Rdx}]=="ServerStart" && fromModule(c.ret,"EXCEL.EXE")) {
            auto params=c.raw[5];auto values=read<Address>(params);
            auto vt=read<uint16_t>(values);
            if(read<uint32_t>(params+16)==1 && read<uint32_t>(params+20)==0 && c.raw[4]==1 && (vt==9 || vt==13) && read<Address>(values+8)) {
                auto candidate=rtdCandidates.at(ctx.Rcx);
                objects[ctx.Rcx]="RTD server";rtdCandidates.erase(ctx.Rcx);
                emit("\"event\":\"rtd_dispatch_binding\",\"object\":"+hex(ctx.Rcx)+",\"clsid\":"+quote(candidate.clsid)+",\"creation_iid\":"+quote(candidate.iid)+",\"xlfRtd_call\":"+std::to_string(candidate.call)+",\"activation_thread\":"+std::to_string(candidate.thread)+",\"server_start_thread\":"+std::to_string(thread)+",\"server_start_dispid\":"+std::to_string((int32_t)ctx.Rdx)+",\"evidence\":\"Excel invoked ServerStart on the exact instance created during xlfRtd\"");
            }
        }
        if(entry.name.rfind("COM!early.",0)==0 && entry.name.rfind("COM!early."+interfaces[ctx.Rcx]+".",0)!=0)return;
        if(entry.name.rfind("COM!early.",0)==0) {
            // Excel's adjustor thunk can tail-forward to a second watched
            // vtable body with exactly the same caller/stack frame. Preserve
            // that identity observation without counting a second API call.
            size_t count=entry.signature=="object1"||entry.signature=="bstr1"||entry.signature=="int1"||entry.signature=="bool1"?1:entry.signature=="object2"||entry.signature=="bstr2"?2:0;
            for(auto& active:calls[thread])if(count&&active.entry.name==entry.name&&active.ret==c.ret&&active.stack==c.stack&&std::equal(c.raw.begin()+1,c.raw.begin()+1+count,active.raw.begin()+1)){
                std::string args="[";for(size_t i=1;i<=count;++i){if(i>1)args+=",";args+=hex(c.raw[i]);}args+="]";
                emit("\"event\":\"com_tail_forward\",\"call\":"+std::to_string(active.id)+",\"entry\":"+quote(entry.name)+",\"thread\":"+std::to_string(thread)+",\"object\":"+hex(ctx.Rcx)+",\"entry_address\":"+hex(ctx.Rip)+",\"stack\":"+hex(ctx.Rsp)+",\"return\":"+hex(c.ret)+",\"non_this_arguments\":"+args);return;
            }
        }
        fields+=",\"object\":"+hex(ctx.Rcx)+",\"object_label\":"+quote(objects[ctx.Rcx]);
        if(entry.name.rfind("COM!early.",0)==0) {
            fields+=",\"result_abi\":"+quote(entry.signature);
            size_t count=entry.signature=="object1"||entry.signature=="bstr1"||entry.signature=="int1"||entry.signature=="bool1"?1:entry.signature=="object2"||entry.signature=="bstr2"?2:0;
            if(count){fields+=",\"stack\":"+hex(ctx.Rsp)+",\"return\":"+hex(c.ret)+",\"entry_address\":"+hex(ctx.Rip)+",\"non_this_arguments\":[";for(size_t i=1;i<=count;++i){if(i>1)fields+=",";fields+=hex(c.raw[i]);}fields+="]";}
            if(entry.signature=="putbool") fields+=",\"value\":"+std::string((int16_t)ctx.Rdx?"true":"false");
            if(entry.signature=="rtdstart"){fields+=",\"callback\":"+hex(ctx.Rdx);watchObject(ctx.Rdx,"RTD update sink","rtdsink");}
            if(entry.signature=="rtdconnect")fields+=",\"topic\":"+std::to_string((int32_t)ctx.Rdx)+",\"strings\":"+variantArray(read<Address>(ctx.R8))+",\"new_values\":"+std::string(read<int16_t>(ctx.R9)?"true":"false");
            if(entry.signature=="rtddisconnect")fields+=",\"topic\":"+std::to_string((int32_t)ctx.Rdx);
            if(entry.signature=="rtdinterval")fields+=",\"value\":"+std::to_string((int32_t)ctx.Rdx);
            if(entry.signature=="addinconnect") {
                fields+=",\"application\":"+hex(ctx.Rdx)+",\"mode\":"+std::to_string((int32_t)ctx.R8)+",\"instance\":"+hex(ctx.R9)+",\"custom\":"+variantArray(read<Address>(c.raw[4]));
                watchObject(ctx.Rdx,"COM add-in Application","application");watchObject(ctx.R9,"COM add-in instance","dispatch");
            }
            if(entry.signature=="addindisconnect")fields+=",\"mode\":"+std::to_string((int32_t)ctx.Rdx)+",\"custom\":"+variantArray(read<Address>(ctx.R8));
            if(entry.signature=="addinarray")fields+=",\"custom\":"+variantArray(read<Address>(ctx.Rdx));
            if(entry.name=="COM!early.ribbon.GetCustomUI")fields+=",\"ribbon_id\":"+quote(utf8(wideZ(ctx.Rdx)));
            if(entry.name=="COM!early.workbooks.Item")fields+=",\"index\":"+variant(ctx.Rdx);
            if(entry.signature=="run") {
                fields+=",\"arguments\":[";
                for(int i=1;i<=31;++i){if(i!=1)fields+=",";fields+=variant(argument(ctx,i));}
                fields+="]";c.run_result=argument(ctx,32);
            }
        }else if(entry.name=="COM!QueryInterface")fields+=",\"iid\":"+quote(guid(ctx.Rdx));
        else if(entry.name=="COM!CreateInstance") {
            c.clsid=factoryClasses[ctx.Rcx];
            for(auto it=calls[thread].rbegin();it!=calls[thread].rend();++it)if(it->fn==379 && it->entry.name.find("!MdCallBack12")!=std::string::npos){c.rtd_call=it->id;break;}
            fields+=",\"iid\":"+quote(guid(ctx.R8))+",\"factory_clsid\":"+quote(c.clsid)+",\"xlfRtd_call\":"+std::to_string(c.rtd_call);
        }
        else if(entry.name=="COM!GetTypeInfo")fields+=",\"index\":"+std::to_string((uint32_t)ctx.Rdx)+",\"lcid\":"+std::to_string((uint32_t)ctx.R8);
        else if(entry.name=="COM!GetIDsOfNames") {
            if(interfaces[ctx.Rcx]=="container"||interfaces[ctx.Rcx]=="point")return;
            fields+=",\"names\":[";
            for(unsigned i=0;i<(unsigned)ctx.R9&&i<64;++i){if(i)fields+=",";auto n=utf8(wideZ(read<Address>(ctx.R8+i*8)));c.names.push_back(n);fields+=quote(n);}
            fields+="]";
        }else if(entry.name=="COM!Invoke") {
            if(interfaces[ctx.Rcx]=="container"||interfaces[ctx.Rcx]=="point")return;
            auto key=std::make_pair(ctx.Rcx,(int32_t)ctx.Rdx);
            fields+=",\"dispid\":"+std::to_string((int32_t)ctx.Rdx)+",\"member\":"+quote(members[key])+",\"flags\":"+std::to_string((uint16_t)c.raw[4]);
            Address params=c.raw[5];Address values=read<Address>(params);uint32_t count=read<uint32_t>(params+16);
            fields+=",\"rgvarg\":[";
            for(unsigned i=0;i<count&&i<256;++i){
                if(i)fields+=",";fields+=variant(values+i*24);
                // Every delivery can contain a fresh Range, even when its vtable
                // matches an earlier object. Release may have retired the old
                // pointer, so discovering only the first delivery loses calls.
                if(objects[ctx.Rcx].rfind("AppEvents sink",0)==0&&(int32_t)ctx.Rdx==0x61c){
                    Address v=values+i*24;uint16_t vt=read<uint16_t>(v);
                    // xlOil casts the declared SheetChange worksheet directly
                    // to _Worksheet, without necessarily querying its IID first.
                    auto kind=i==0?"worksheet":"range";
                    if(vt==9)watchObject(read<Address>(v+8),"AppEvents rgvarg["+std::to_string(i)+"]",kind);
                    else if(vt==0x4009)watchObject(read<Address>(read<Address>(v+8)),"AppEvents rgvarg["+std::to_string(i)+"]",kind);
                }
                if(i==0&&objects[ctx.Rcx]=="RTD server"&&(int32_t)ctx.Rdx==10){Address v=values;auto vt=read<uint16_t>(v);if(vt==9||vt==13)watchObject(read<Address>(v+8),"RTD update callback","unknown");}
            }
            fields+="],\"named_dispids\":[";
            Address named=read<Address>(params+8);uint32_t countNamed=read<uint32_t>(params+20);
            for(unsigned i=0;i<countNamed&&i<256;++i){if(i)fields+=",";fields+=std::to_string(read<int32_t>(named+i*4));}
            fields+="]";
        }else if(entry.name=="COM!FindConnectionPoint")fields+=",\"iid\":"+quote(guid(ctx.Rdx));
        else if(entry.name=="COM!Advise"){fields+=",\"sink\":"+hex(ctx.Rdx);watchObject(ctx.Rdx,"AppEvents sink","unknown");}
        else if(entry.name=="COM!Unadvise")fields+=",\"cookie\":"+std::to_string((uint32_t)ctx.Rdx);
    }else if(entry.name.find("!DllGetClassObject")!=std::string::npos) {
        fields+=",\"clsid\":"+quote(guid(ctx.Rcx))+",\"iid\":"+quote(guid(ctx.Rdx));
    }else if(entry.name.find("!AccessibleObjectFromWindow")!=std::string::npos) {
        fields+=",\"hwnd\":"+hex(ctx.Rcx)+",\"object_id\":"+std::to_string((int32_t)ctx.Rdx)+",\"iid\":"+quote(guid(ctx.R8));
    }else if(entry.name.find("!LPenHelper")!=std::string::npos) {
        fields+=",\"function\":"+std::to_string((int32_t)ctx.Rcx);
    }else if(entry.name.find("!xlAutoFree12")!=std::string::npos) {
        fields+=",\"oper_address\":"+hex(ctx.Rcx)+",\"args\":"+entryArgs(entry,ctx);
    }else if(entry.name.find("!MdCallBack12")!=std::string::npos) {
        c.fn=(int)ctx.Rcx; c.count=(int)ctx.Rdx;
        fields+=",\"function\":"+std::to_string(c.fn)+",\"args\":[";
        for(int i=0;i<c.count&&i<256;++i) { if(i)fields+=","; fields+=oper(read<Address>(c.args+i*8)); }
        fields+="],\"argument_addresses\":[";
        for(int i=0;i<c.count&&i<256;++i){if(i)fields+=",";fields+=hex(read<Address>(c.args+i*8));}
        fields+="],\"result_address\":"+hex(c.result);
        registerEntry(c);
    } else fields+=",\"signature\":"+quote(entry.signature)+",\"args\":"+entryArgs(entry,ctx);
    emit(fields);
    calls[thread].push_back(c); arm(c.ret);++breakpoints[c.ret].returnRefs;
}
static void leave(const Call& c,const CONTEXT& ctx) {
    std::string fields="\"event\":\"leave\",\"call\":"+std::to_string(c.id)+",\"thread\":"+std::to_string(c.thread)+",\"entry\":"+quote(c.entry.name)+",\"elapsed_qpc\":"+std::to_string(ticks()-c.started)+",\"rax\":"+hex(ctx.Rax);
    if(c.entry.name.find("!CoCreateInstance")!=std::string::npos){
        fields+=",\"hresult\":"+std::to_string((int32_t)ctx.Rax);
        if((int32_t)ctx.Rax>=0){auto value=read<Address>(c.raw[4]);fields+=",\"object\":"+hex(value);watchObject(value,"RTD server","rtdserver");}
    }else if(c.entry.name.find("!CoRegisterClassObject")!=std::string::npos){
        fields+=",\"hresult\":"+std::to_string((int32_t)ctx.Rax);
    }else if(c.entry.name.find("!DllGetClassObject")!=std::string::npos){fields+=",\"hresult\":"+std::to_string((int32_t)ctx.Rax);if((int32_t)ctx.Rax>=0){auto factory=read<Address>(c.raw[2]);watchObject(factory,"DLL class factory","classfactory");factoryClasses[factory]=guid(c.raw[0]);}
    }else if(c.entry.name.find("!AccessibleObjectFromWindow")!=std::string::npos) {
        fields+=",\"hresult\":"+std::to_string((int32_t)ctx.Rax)+",\"object\":"+hex(read<Address>(c.raw[3]));
        if((int32_t)ctx.Rax>=0)watchObject(read<Address>(c.raw[3]),"AccessibleObjectFromWindow");
    }else if(c.entry.name.rfind("COM!",0)==0) {
        if(c.entry.name=="COM!Release") {
            fields+=",\"references\":"+std::to_string((uint32_t)ctx.Rax);
            if((uint32_t)ctx.Rax==0)forgetObject(c.raw[0]);
        } else fields+=",\"hresult\":"+std::to_string((int32_t)ctx.Rax);
        if(c.entry.name!="COM!Release" && (int32_t)ctx.Rax>=0){
            std::string kind=c.entry.name;
            if(kind.rfind("COM!early.",0)==0) {
                const auto& abi=c.entry.signature;
                Address out=abi.back()=='2'?c.raw[2]:c.raw[1];
                if(abi.rfind("object",0)==0) {
                    Address value=read<Address>(out);fields+=",\"object\":"+hex(value);
                    std::string target="dispatch";
                    if(kind=="COM!early.application.Workbooks")target="workbooks";
                    else if(kind=="COM!early.application.ActiveWorkbook" || kind=="COM!early.workbooks.Item")target="workbook";
                    else if(kind=="COM!early.application.ActiveSheet" || kind=="COM!early.workbook.ActiveSheet")target="worksheet";
                    else if(kind.size()>=12 && kind.substr(kind.size()-12)==".Application")target="application";
                    watchObject(value,kind,target);
                } else if(abi.rfind("bstr",0)==0) {
                    Address value=read<Address>(out);fields+=",\"value\":"+(value?quote(utf8(wide(value,read<uint32_t>(value-4)/2))):"null");
                } else if(abi=="int1")fields+=",\"value\":"+std::to_string(read<int32_t>(out));
                else if(abi=="bool1")fields+=",\"value\":"+std::string(read<int16_t>(out)?"true":"false");
                else if(abi=="run")fields+=",\"result\":"+variant(c.run_result);
                else if(abi=="rtdstart")fields+=",\"started\":"+std::to_string(read<int32_t>(c.raw[2]));
                else if(abi=="rtdconnect")fields+=",\"result\":"+variant(c.raw[4])+",\"new_values\":"+std::string(read<int16_t>(c.raw[3])?"true":"false");
                else if(abi=="rtdrefresh")fields+=",\"count\":"+std::to_string(read<int32_t>(c.raw[1]))+",\"data\":"+variantArray(read<Address>(c.raw[2]));
            }else if(kind=="COM!GetTypeInfoCount")fields+=",\"count\":"+std::to_string(read<uint32_t>(c.raw[1]));
            else if(kind=="COM!GetTypeInfo")fields+=",\"type_info\":"+hex(read<Address>(c.raw[3]));
            else if(kind=="COM!CreateInstance"){
                auto iid=guid(c.raw[2]);auto object=read<Address>(c.raw[3]);fields+=",\"object\":"+hex(object);
                bool rtd=iid=="ec0e6191-db51-11d3-8f3e-00c04f3651b8";
                watchObject(object,rtd?"RTD server":"COM class instance",rtd?"rtdserver":iid=="b65ad801-abaf-11d0-bb8b-00a0c90f2744"?"extensibility":iid=="00020400-0000-0000-c000-000000000046"?"dispatch":"unknown");
                instanceClasses[object]=c.clsid;
                if(c.rtd_call && !rtd)rtdCandidates[object]={c.rtd_call,c.thread,c.clsid,iid};
            }else if(kind=="COM!GetIDsOfNames") {
                fields+=",\"dispids\":[";
                for(size_t i=0;i<c.names.size();++i){int32_t id=read<int32_t>(c.raw[5]+i*4);members[{c.raw[0],id}]=c.names[i];if(i)fields+=",";fields+=std::to_string(id);}fields+="]";
            }else if(kind=="COM!Invoke") {
                fields+=",\"result\":"+variant(c.raw[6]);
                if(objects[c.raw[0]]=="RTD server") {auto values=read<Address>(c.raw[5]);auto count=read<uint32_t>(c.raw[5]+16);fields+=",\"arguments_after\":[";for(uint32_t i=0;i<count&&i<256;++i){if(i)fields+=",";fields+=variant(values+i*24);}fields+="]";}
                if(c.raw[6]&&read<uint16_t>(c.raw[6])==9)
                    watchObject(read<Address>(c.raw[6]+8),objects[c.raw[0]]+"."+members[{c.raw[0],(int32_t)c.raw[1]}]);
            }else if(kind=="COM!QueryInterface") {
                std::string iid=guid(c.raw[1]);Address p=read<Address>(c.raw[2]);fields+=",\"object\":"+hex(p);
                if(iid=="00020400-0000-0000-c000-000000000046")watchObject(p,objects[c.raw[0]]);
                else if(iid=="00024413-0000-0000-c000-000000000046")watchObject(p,objects[c.raw[0]],"sink");
                else if(iid=="b196b284-bab4-101a-b69c-00aa00341d07")watchObject(p,objects[c.raw[0]]+".ConnectionPointContainer","container");
                else if(iid=="000208d5-0000-0000-c000-000000000046")watchObject(p,objects[c.raw[0]],"application");
                else if(iid=="000208da-0000-0000-c000-000000000046")watchObject(p,objects[c.raw[0]],"workbook");
                else if(iid=="000208db-0000-0000-c000-000000000046")watchObject(p,objects[c.raw[0]],"workbooks");
                else if(iid=="000208d8-0000-0000-c000-000000000046")watchObject(p,objects[c.raw[0]],"worksheet");
                else if(iid=="00020846-0000-0000-c000-000000000046")watchObject(p,objects[c.raw[0]],"range");
                else if(iid=="ec0e6191-db51-11d3-8f3e-00c04f3651b8")watchObject(p,objects[c.raw[0]],"rtdserver");
                else if(iid=="a43788c1-d91b-11d3-8f39-00c04f3651b8")watchObject(p,objects[c.raw[0]],"rtdsink");
                else if(iid=="b65ad801-abaf-11d0-bb8b-00a0c90f2744")watchObject(p,objects[c.raw[0]],"extensibility");
                else if(iid=="000c0396-0000-0000-c000-000000000046")watchObject(p,objects[c.raw[0]],"ribbon");
                else watchObject(p,objects[c.raw[0]],"unknown");
            }else if(kind=="COM!FindConnectionPoint")watchObject(read<Address>(c.raw[2]),objects[c.raw[0]]+".ConnectionPoint","point");
            else if(kind=="COM!Advise"){
                fields+=",\"cookie\":"+std::to_string(read<uint32_t>(c.raw[2]));
            }
        }
    }else if(c.entry.name.find("!LPenHelper")!=std::string::npos) {
        // xlGetFmlaInfo's first LONG is the edit-mode value, observed before and after.
        fields+=",\"output_first_long\":"+std::to_string(read<int32_t>(c.raw[1]));
    }else if(c.entry.name.find("!MdCallBack12")!=std::string::npos) {
        fields+=",\"function\":"+std::to_string(c.fn)+",\"return_code\":"+std::to_string((int32_t)ctx.Rax)+",\"result_valid\":"+((int32_t)ctx.Rax==0?"true":"false")+",\"result\":"+oper(c.result);
    } else {
        auto parts=signatureParts(c.entry.signature);
        if(!parts.empty()) {
            fields+=",\"result\":";
            if(parts[0]=="B") { double d; memcpy(&d,&ctx.Xmm0,8); fields+=number(d); }
            else if(parts[0]==">")fields+="null";
            else fields+=typed(parts[0],ctx.Rax);
        }
    }
    emit(fields);
}
static bool holdThread(HANDLE h,DWORD thread) {
    ULONG previous=0;LONG status=ntSuspendThread(h,&previous);
    if(status>=0)return true;
    if((uint32_t)status==0xc000004b){
        // STATUS_THREAD_IS_TERMINATING: no further user instructions can run.
        // Preserve the native status; Win32 maps this and access denial to error 5.
        emit("\"event\":\"thread_terminating\",\"thread\":"+std::to_string(thread)+",\"ntstatus\":"+hex((uint32_t)status));return false;
    }
    healthy=false;emit("\"event\":\"debugger_error\",\"operation\":\"NtSuspendThread\",\"thread\":"+std::to_string(thread)+",\"ntstatus\":"+hex((uint32_t)status));return false;
}
static void suspendOthers(DWORD current) {
    // DebugActiveProcess reports the initial thread and every existing thread,
    // then every creation/exit before that thread can execute user instructions.
    // Keep that exact inventory instead of taking a system-wide snapshot at
    // every breakpoint. Snapshot cost stretched the RTD observation interval.
    // https://learn.microsoft.com/windows/win32/debug/debugging-events
    for(DWORD thread:liveThreads) {
        if(thread==current||exitedThreads.count(thread))continue;
        HANDLE h=OpenThread(THREAD_SUSPEND_RESUME|THREAD_QUERY_LIMITED_INFORMATION,FALSE,thread);
        if(h) {
            DWORD owner=GetProcessIdOfThread(h);
            if(owner!=processId){
                if(!owner)debuggerError("GetProcessIdOfThread");
                CloseHandle(h);continue;
            }
            if(holdThread(h,thread))suspended[current].push_back(h);
            else CloseHandle(h);
        }
        else debuggerError("OpenThread single step");
    }
}
static void resumeOthers(DWORD thread) {
    for(HANDLE h:suspended[thread]) {
        DWORD id=GetThreadId(h);
        if(!id)debuggerError("GetThreadId before resume");
        else if(!exitedThreads.count(id)&&ResumeThread(h)==DWORD(-1))debuggerError("ResumeThread");
        CloseHandle(h);
    }
    suspended.erase(thread);
}
static void finishStep(DWORD thread) {
    Address addr=stepping[thread];
    auto it=breakpoints.find(addr);
    if(it!=breakpoints.end()) {
        if(!it->second.entries.empty()||it->second.returnRefs)arm(addr);
        // Other threads may already have trapped on this byte before it was
        // restored. Keep the disabled record so their queued debug events can
        // still rewind and execute the original instruction safely.
    }
    stepping.erase(thread);resumeOthers(thread);
}
static bool breakpoint(DWORD thread,Address addr) {
    auto found=breakpoints.find(addr); if(found==breakpoints.end())return false;
    HANDLE h=OpenThread(THREAD_GET_CONTEXT|THREAD_SET_CONTEXT,FALSE,thread);
    CONTEXT ctx{}; ctx.ContextFlags=CONTEXT_ALL;
    if(!h||!GetThreadContext(h,&ctx)) { healthy=false; if(h)CloseHandle(h); return false; }
    ctx.Rip=addr;
    // A queued breakpoint from another thread can precede the single-step event.
    // Rewind that thread and keep it suspended until the current step completes.
    // Nesting suspend-all operations here would deadlock calculation workers.
    if(!stepping.empty()&&!stepping.count(thread)) {
        DWORD owner=stepping.begin()->first;
        bool alreadySuspended=false;
        for(HANDLE other:suspended[owner])if(GetThreadId(other)==thread)alreadySuspended=true;
        if(!alreadySuspended) {
            HANDLE other=OpenThread(THREAD_SUSPEND_RESUME|THREAD_QUERY_LIMITED_INFORMATION,FALSE,thread);
            if(other&&holdThread(other,thread))suspended[owner].push_back(other);
            else {if(!other)debuggerError("OpenThread deferred breakpoint");if(other)CloseHandle(other);}
        }
        if(!SetThreadContext(h,&ctx))debuggerError("SetThreadContext deferred breakpoint");CloseHandle(h);return true;
    }
    auto& pending=calls[thread];
    for(size_t i=pending.size();i>0;--i) {
        if(pending[i-1].ret==addr&&pending[i-1].stack+8==ctx.Rsp) {
            leave(pending[i-1],ctx); pending.erase(pending.begin()+i-1);
            if(found->second.returnRefs)--found->second.returnRefs;
            // COM adjustor thunks can tail-forward through another watched vtable
            // entry with this same return frame. Both observations return here.
        }
    }
    // One machine address can be re-registered under a new name. Latest registration wins.
    if(!found->second.entries.empty()) {
        const Entry* selected=&found->second.entries.back();
        if(selected->name.rfind("COM!early.",0)==0) {
            auto prefix="COM!early."+interfaces[ctx.Rcx]+".";
            for(auto it=found->second.entries.rbegin();it!=found->second.entries.rend();++it)
                if(it->name.rfind(prefix,0)==0){selected=&*it;break;}
        }
        enter(*selected,thread,ctx);
    }
    suspendOthers(thread);
    if(!writeByte(addr,found->second.original))healthy=false;
    found->second.enabled=false; stepping[thread]=addr;
    ctx.EFlags|=0x100;if(!SetThreadContext(h,&ctx))debuggerError("SetThreadContext single step");CloseHandle(h);return true;
}
static std::wstring filePath(HANDLE h) {
    wchar_t path[32768]; DWORD n=GetFinalPathNameByHandleW(h,path,32768,FILE_NAME_NORMALIZED);
    if(!n||n>=32768)return L""; return std::wstring(path,n);
}
static void stopAtWatchedBreakpoint(DWORD thread,Address addr) {
    HANDLE h=OpenThread(THREAD_GET_CONTEXT|THREAD_SET_CONTEXT|THREAD_SUSPEND_RESUME|THREAD_QUERY_LIMITED_INFORMATION,FALSE,thread);
    CONTEXT ctx{};ctx.ContextFlags=CONTEXT_CONTROL;
    if(!h||!GetThreadContext(h,&ctx)){healthy=false;if(h)CloseHandle(h);return;}
    ctx.Rip=addr;ctx.EFlags&=~0x100;
    if(!SetThreadContext(h,&ctx))debuggerError("SetThreadContext stop rewind");
    auto& b=breakpoints[addr];
    if(b.enabled){if(!writeByte(addr,b.original))healthy=false;b.enabled=false;}
    if(holdThread(h,thread))stoppedThreads.push_back(h);else CloseHandle(h);
    emit("\"event\":\"stop_breakpoint_rewound\",\"thread\":"+std::to_string(thread)+",\"address\":"+hex(addr));
}
static void cleanup() {
    size_t restored=0, failed=0;
    for(auto& [addr,b]:breakpoints) if(b.enabled) { if(writeByte(addr,b.original))++restored; else ++failed; }
    for(auto& [thread,addr]:stepping) {
        HANDLE h=OpenThread(THREAD_GET_CONTEXT|THREAD_SET_CONTEXT,FALSE,thread);
        if(h){ CONTEXT c{};c.ContextFlags=CONTEXT_CONTROL;if(GetThreadContext(h,&c)){c.EFlags&=~0x100;if(!SetThreadContext(h,&c))debuggerError("SetThreadContext cleanup");}else debuggerError("GetThreadContext cleanup");CloseHandle(h); }
    }
    while(!suspended.empty())resumeOthers(suspended.begin()->first);
    for(HANDLE h:stoppedThreads){DWORD id=GetThreadId(h);if(!id)debuggerError("GetThreadId stop hold");else if(!exitedThreads.count(id)&&ResumeThread(h)==DWORD(-1))debuggerError("ResumeThread stop hold");CloseHandle(h);}stoppedThreads.clear();
    size_t outstanding=0;for(auto& [t,v]:calls)outstanding+=v.size();
    healthy=healthy&&failed==0&&outstanding==0;
    emit("\"event\":\"detach\",\"restored\":"+std::to_string(restored)+",\"restore_failed\":"+std::to_string(failed)+",\"outstanding_calls\":"+std::to_string(outstanding)+",\"healthy\":"+(healthy?"true":"false"));
}
int wmain(int argc,wchar_t** argv) {
    if(argc!=4&&argc!=5){ fwprintf(stderr,L"Usage: trace.exe OWNED_PID output.jsonl stop-file [exact-owned-host-image]\n"); return 2; }
    processId=wcstoul(argv[1],nullptr,10);
    if(processId==3144){fputs("PID 3144 is excluded\n",stderr);return 2;}
    process=OpenProcess(PROCESS_ALL_ACCESS,FALSE,processId);
    if(!process){fprintf(stderr,"OpenProcess: %lu\n",GetLastError());return 3;}
    wchar_t image[32768]; DWORD size=32768;
    wchar_t expected[32768];DWORD expectedSize=argc==5?GetFullPathNameW(argv[4],32768,expected,nullptr):0;
    bool readImage=QueryFullProcessImageNameW(process,0,image,&size)!=FALSE;
    bool correctImage=readImage && (argc==5 ? expectedSize>0&&expectedSize<32768&&_wcsicmp(image,expected)==0
        : _stricmp(basename(image).c_str(),"EXCEL.EXE")==0);
    if(!readImage||!correctImage) {
        fputs("Target must be Excel or the explicitly named owned host image\n",stderr);CloseHandle(process);return 3;
    }
    output.open(argv[2],std::ios::binary); if(!output){CloseHandle(process);return 3;}
    ntSuspendThread=(NtSuspendThreadFn)GetProcAddress(GetModuleHandleW(L"ntdll.dll"),"NtSuspendThread");
    if(!ntSuspendThread){CloseHandle(process);return 3;}
    QueryPerformanceFrequency(&frequency);
    if(!DebugActiveProcess(processId)){fprintf(stderr,"DebugActiveProcess: %lu\n",GetLastError());CloseHandle(process);return 4;}
    if(!DebugSetProcessKillOnExit(FALSE))debuggerError("DebugSetProcessKillOnExit");
    emit("\"event\":\"attach\",\"pid\":"+std::to_string(processId)+",\"qpc_frequency\":"+std::to_string(frequency.QuadPart)+",\"architecture\":\"x64\"");
    bool stopping=false,initialBreak=true,done=false;
    while(!done){
        if(!stopping&&GetFileAttributesW(argv[3])!=INVALID_FILE_ATTRIBUTES){stopping=true;if(!DebugBreakProcess(process))debuggerError("DebugBreakProcess");}
        DEBUG_EVENT ev{};
        if(!WaitForDebugEvent(&ev,100))continue;
        DWORD status=DBG_CONTINUE;
        switch(ev.dwDebugEventCode){
        case CREATE_PROCESS_DEBUG_EVENT:{liveThreads.insert(ev.dwThreadId);auto& p=ev.u.CreateProcessInfo;addModule((Address)p.lpBaseOfImage,filePath(p.hFile));if(p.hFile)CloseHandle(p.hFile);break;}
        case CREATE_THREAD_DEBUG_EVENT:{
            // Process/thread handles supplied by debug events belong to the debugger
            // lifecycle: ContinueDebugEvent closes them on exit. Never close or retain
            // those values as owned holds; open an independent handle when needed.
            exitedThreads.erase(ev.dwThreadId);liveThreads.insert(ev.dwThreadId);
            bool debugBreakThread=false;
            if(stopping)for(auto& [base,m]:modules){auto found=m.exports.find("DbgUiRemoteBreakin");if(found!=m.exports.end()&&found->second==(Address)ev.u.CreateThread.lpStartAddress)debugBreakThread=true;}
            if(!stepping.empty()&&(!stopping||!debugBreakThread)){
                HANDLE h=OpenThread(THREAD_SUSPEND_RESUME|THREAD_QUERY_LIMITED_INFORMATION,FALSE,ev.dwThreadId);
                if(!h)debuggerError("OpenThread creation hold");
                else if(holdThread(h,ev.dwThreadId)){
                    if(stopping)stoppedThreads.push_back(h);else suspended[stepping.begin()->first].push_back(h);
                }else CloseHandle(h);
            }
            break;
        }
        case LOAD_DLL_DEBUG_EVENT:{auto& d=ev.u.LoadDll;addModule((Address)d.lpBaseOfDll,filePath(d.hFile));if(d.hFile)CloseHandle(d.hFile);break;}
        case UNLOAD_DLL_DEBUG_EVENT:{Address base=(Address)ev.u.UnloadDll.lpBaseOfDll;auto it=modules.find(base);if(it!=modules.end()){emit("\"event\":\"module_unload\",\"base\":"+hex(base)+",\"name\":"+quote(basename(it->second.path))+",\"path\":"+quote(utf8(it->second.path)));Address end=base+it->second.size;for(auto b=breakpoints.lower_bound(base);b!=breakpoints.end()&&b->first<end;)b=breakpoints.erase(b);modules.erase(it);}break;}
        case EXIT_THREAD_DEBUG_EVENT:liveThreads.erase(ev.dwThreadId);exitedThreads.insert(ev.dwThreadId);emit("\"event\":\"thread_exit\",\"thread\":"+std::to_string(ev.dwThreadId)+",\"exit_code\":"+std::to_string(ev.u.ExitThread.dwExitCode));if(stepping.count(ev.dwThreadId))finishStep(ev.dwThreadId);if(!calls[ev.dwThreadId].empty())healthy=false;break;
        case EXCEPTION_DEBUG_EVENT:{
            DWORD code=ev.u.Exception.ExceptionRecord.ExceptionCode;
            Address addr=(Address)ev.u.Exception.ExceptionRecord.ExceptionAddress;
            if(code==EXCEPTION_SINGLE_STEP&&stepping.count(ev.dwThreadId)){
                finishStep(ev.dwThreadId);
            }else if(code==EXCEPTION_BREAKPOINT){
                if(stopping&&breakpoints.count(addr))stopAtWatchedBreakpoint(ev.dwThreadId,addr);
                else if(stopping){cleanup();done=true;}
                else if(!breakpoint(ev.dwThreadId,addr)){if(initialBreak){initialBreak=false;emit("\"event\":\"ready\"");}else {healthy=false;emit("\"event\":\"unrecognized_breakpoint\",\"address\":"+hex(addr)+",\"thread\":"+std::to_string(ev.dwThreadId)+",\"first_chance\":"+(ev.u.Exception.dwFirstChance?"true":"false"));status=DBG_EXCEPTION_NOT_HANDLED;}}
            }else {
                status=DBG_EXCEPTION_NOT_HANDLED;
                std::string parameters="[";
                for(DWORD i=0;i<ev.u.Exception.ExceptionRecord.NumberParameters&&i<EXCEPTION_MAXIMUM_PARAMETERS;++i){if(i)parameters+=",";parameters+=hex(ev.u.Exception.ExceptionRecord.ExceptionInformation[i]);}
                parameters+="]";
                emit("\"event\":\"exception\",\"code\":"+hex(code)+",\"address\":"+hex(addr)+",\"thread\":"+std::to_string(ev.dwThreadId)+",\"first_chance\":"+(ev.u.Exception.dwFirstChance?"true":"false")+",\"parameters\":"+parameters);
                if(!ev.u.Exception.dwFirstChance){healthy=false;emit("\"event\":\"second_chance_exception\",\"code\":"+hex(code)+",\"address\":"+hex(addr)+",\"thread\":"+std::to_string(ev.dwThreadId));}
            }
            break;
        }
        case EXIT_PROCESS_DEBUG_EVENT:healthy=false;emit("\"event\":\"process_exit\",\"code\":"+std::to_string(ev.u.ExitProcess.dwExitCode));done=true;break;
        }
        if(!ContinueDebugEvent(ev.dwProcessId,ev.dwThreadId,status)){debuggerError("ContinueDebugEvent");done=true;}
    }
    if(!DebugActiveProcessStop(processId))debuggerError("DebugActiveProcessStop");CloseHandle(process);return healthy?0:5;
}
