// Measurement-only Windows x64 XLL. Written here against the documented C API.
// ABI/constants: https://learn.microsoft.com/office/client-developer/excel/data-types-used-by-excel
#include <windows.h>
#include <atomic>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

struct CellRect { int r0, r1, c0, c1; };
struct MultiRef { unsigned short count; CellRect rects[1]; };
struct Operand {
    union Payload {
        double number;
        wchar_t* text;
        int integer;
        struct { unsigned short count; CellRect rect; } single;
        struct { MultiRef* refs; uintptr_t sheet; } reference;
        struct { Operand* cells; int rows, columns; } array;
        struct { void* bytes; long count; } binary;
        unsigned char padding[24];
    } value;
    unsigned long type;
};
static_assert(sizeof(Operand) == 32 && offsetof(Operand, type) == 24);
using Callback = int (WINAPI*)(int, int, Operand**, Operand*);
static Callback excel;
static constexpr int freeFn=0x4000, coerceFn=0x4002, nameFn=0x4009, asyncFn=0x4010;
static constexpr int registerFn=149, callerFn=89, volatileFn=237, evaluateFn=257;
static HMODULE selfModule;
struct Record { unsigned long thread, tick; long seq; int kind, site, status; unsigned long type; double value; uintptr_t handle; };
static Record records[100000];
static std::atomic<long> used{0};
static std::atomic<int> entries[16];
static void log(int kind, int site, int status, const Operand* v=nullptr, uintptr_t handle=0) {
    long i=used.fetch_add(1);
    if(i>=100000) return;
    Record r{}; r.thread=GetCurrentThreadId(); r.tick=GetTickCount(); r.seq=i;
    r.kind=kind; r.site=site; r.status=status; r.handle=handle;
    if(v) { r.type=v->type; auto type=v->type&0xfff;
        if(type==1) r.value=v->value.number;
        else if(type==0x800 || type==4 || type==16) r.value=v->value.integer; }
    records[i]=r;
}
static int call(int fn, Operand* result, std::initializer_list<Operand*> args={}) {
    std::vector<Operand*> list(args);
    return excel ? excel(fn,(int)list.size(),list.data(),result) : 32;
}
static Operand num(double d) { Operand r{}; r.type=1; r.value.number=d; return r; }
static Operand integer(int i) { Operand r{}; r.type=0x800; r.value.integer=i; return r; }
struct Text {
    std::vector<wchar_t> data; Operand value{};
    explicit Text(const wchar_t* s) { size_t n=wcslen(s); data.resize(n+1); data[0]=(wchar_t)n; memcpy(data.data()+1,s,n*2); value.type=2; value.value.text=data.data(); }
};
static double countRead(Operand* ref, int site, bool nonvolatile) {
    entries[site]++;
    log(1,site,entries[site].load(),ref);
    if(nonvolatile) { Operand f{}; f.type=4; f.value.integer=0; int rc=call(volatileFn,nullptr,{&f}); log(6,site,rc); }
    Operand value{};
    int rc=call(coerceFn,&value,{ref});
    log(2,site,rc,rc==0?&value:nullptr);
    // After uncalculated/abort, do no Excel callback except freeing memory.
    if(rc==64 || rc==1) { log(3,site,rc); return 0; }
    double d=(value.type&0xfff)==1?value.value.number:(value.type&0xfff)==0x800?value.value.integer:0;
    if(rc==0) call(freeFn,nullptr,{&value});
    log(4,site,rc);
    return d;
}
extern "C" __declspec(dllexport) double WINAPI XOrd(Operand* r) { return countRead(r,1,false); }
extern "C" __declspec(dllexport) double WINAPI XMacro(Operand* r) { return countRead(r,2,false); }
extern "C" __declspec(dllexport) double WINAPI XMacroNV(Operand* r) { return countRead(r,3,true); }
extern "C" __declspec(dllexport) double WINAPI XThread(Operand* r) { return countRead(r,4,false); }
extern "C" __declspec(dllexport) double WINAPI XInside() {
    Text expr(L"S!B1"); Operand ref{}; int rc=call(evaluateFn,&ref,{&expr.value}); log(7,5,rc,&ref);
    if(rc==64 || rc==1) return 0;
    double d=rc==0?countRead(&ref,5,false):0;
    if(rc==0) call(freeFn,nullptr,{&ref});
    return d;
}
extern "C" __declspec(dllexport) double WINAPI XBefore(double v) { log(10,10,0); return v; }
extern "C" __declspec(dllexport) double WINAPI XAfter(double v) { log(11,11,0); return v; }

struct Pending { Operand handle{}; std::vector<unsigned char> handleBytes; double argument; HANDLE release; int site; bool duplicate; };
static SRWLOCK pendingLock=SRWLOCK_INIT;
static std::vector<Pending*> pending;
static std::atomic<int> nextSite{0};
static DWORD WINAPI complete(void* arg) {
    auto p=(Pending*)arg;
    DWORD wait=WaitForSingleObject(p->release,30000);
    log(22,p->site,(int)wait,nullptr,(uintptr_t)p->handle.value.binary.bytes);
    if(wait==WAIT_OBJECT_0) {
        Operand result=num(p->argument), answer{};
        int rc=call(asyncFn,&answer,{&p->handle,&result});
        log(23,p->site,rc,&answer,(uintptr_t)p->handle.value.binary.bytes);
        if(p->duplicate) { rc=call(asyncFn,&answer,{&p->handle,&result}); log(24,p->site,rc,&answer,(uintptr_t)p->handle.value.binary.bytes); }
    }
    return 0;
}
extern "C" __declspec(dllexport) void WINAPI AsyncEcho(double value, double latch, Operand* handle) {
    auto p=new Pending(); p->handle=*handle; p->argument=value; p->site=++nextSite; p->duplicate=latch<0;
    if((handle->type&0xfff)==0x802 && handle->value.binary.count>0) {
        auto first=(unsigned char*)handle->value.binary.bytes;
        p->handleBytes.assign(first, first+handle->value.binary.count);
        p->handle.value.binary.bytes=p->handleBytes.data();
    }
    p->release=CreateEventW(nullptr,TRUE,FALSE,nullptr);
    AcquireSRWLockExclusive(&pendingLock); pending.push_back(p); ReleaseSRWLockExclusive(&pendingLock);
    log(20,p->site,(int)latch,handle,(uintptr_t)handle->value.binary.bytes);
    HANDLE t=CreateThread(nullptr,0,complete,p,0,nullptr); if(t) CloseHandle(t);
    log(21,p->site,0);
}
extern "C" __declspec(dllexport) int WINAPI ReleaseCalls(int newestFirst) {
    AcquireSRWLockShared(&pendingLock);
    if(newestFirst) { for(auto i=pending.rbegin();i!=pending.rend();++i) SetEvent((*i)->release); }
    else { for(auto p:pending) SetEvent(p->release); }
    int n=(int)pending.size(); ReleaseSRWLockShared(&pendingLock); return n;
}
extern "C" __declspec(dllexport) int WINAPI DumpNative(const wchar_t* path) {
    FILE* f=nullptr; _wfopen_s(&f,path,L"w"); if(!f) return -1;
    fprintf(f,"sequence\tthread\ttick\tkind\tsite\tstatus\ttype\tvalue\tbits\thandle\n");
    long n=used.load(); if(n>100000)n=100000;
    for(long i=0;i<n;i++) { auto r=records[i]; unsigned long long bits=0; memcpy(&bits,&r.value,8);
        fprintf(f,"%ld\t%lu\t%lu\t%d\t%d\t%d\t%lu\t%.17g\t%016llX\t%p\n",r.seq,r.thread,r.tick,r.kind,r.site,r.status,r.type,r.value,bits,(void*)r.handle); }
    fclose(f); return (int)n;
}
extern "C" __declspec(dllexport) int WINAPI ResetNative() { used=0; for(auto& e:entries)e=0; return 1; }
extern "C" __declspec(dllexport) int WINAPI MarkNative(int stage) { log(99,stage,0); return 1; }
static void reg(const wchar_t* procedure,const wchar_t* types,const wchar_t* formula) {
    Operand name{}, result{}, macro=integer(1); call(nameFn,&name);
    Text proc(procedure),type(types),fn(formula),args(L"input,latch");
    int rc=call(registerFn,&result,{&name,&proc.value,&type.value,&fn.value,&args.value,&macro});
    log(0,0,rc,&result); call(freeFn,nullptr,{&name});
}
extern "C" __declspec(dllexport) int WINAPI xlAutoOpen() {
    if(!excel)excel=(Callback)GetProcAddress(GetModuleHandleW(nullptr),"MdCallBack12");
    if(!excel)return 0;
    reg(L"XOrd",L"BU",L"XOrd"); reg(L"XMacro",L"BU#",L"XMacro");
    reg(L"XMacroNV",L"BU#",L"XMacroNV"); reg(L"XThread",L"BU$",L"XThread");
    reg(L"XInside",L"B#",L"XInside");
    reg(L"XBefore",L"BB",L"XBefore"); reg(L"XAfter",L"BB",L"XAfter");
    reg(L"AsyncEcho",L">BBX",L"AsyncEcho");
    return 1;
}
extern "C" __declspec(dllexport) int WINAPI xlAutoClose() { ReleaseCalls(0); return 1; }
extern "C" __declspec(dllexport) void WINAPI SetExcel12EntryPt(void* p) { excel=(Callback)p; }
BOOL WINAPI DllMain(HINSTANCE h,DWORD why,void*) { if(why==DLL_PROCESS_ATTACH)selfModule=h; return TRUE; }
