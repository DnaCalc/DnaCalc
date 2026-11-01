// Reuse P03's ABI, in-memory logger and worker; keep the P03 measured image intact.
#define xlAutoOpen BaseAutoOpen
#define XBefore BaseXBefore
#define XAfter BaseXAfter
#include "../P03/native-probe.cpp"
#undef xlAutoOpen
#undef XBefore
#undef XAfter

extern "C" __declspec(dllexport) double WINAPI XBefore(double value) { auto v=num(value);log(10,10,0,&v);return value; }
extern "C" __declspec(dllexport) double WINAPI XAfter(double value) { auto v=num(value);log(11,11,0,&v);return value; }
static std::atomic<int> activeCallbacks{0};
static std::vector<HANDLE> entrySignals;
static DWORD WINAPI completeTracked(void* value) { auto rc=complete(value);activeCallbacks--;return rc; }
extern "C" __declspec(dllexport) int WINAPI PendingAsync() {return activeCallbacks.load();}

extern "C" __declspec(dllexport) void WINAPI AsyncProbe(double value,double latch,Operand* handle) {
    auto p=new Pending();
    // The handle's binary union contains an opaque handle, not a byte buffer to dereference.
    // All computational arguments are doubles copied by value before returning.
    p->handle=*handle; p->argument=value; p->site=++nextSite; p->duplicate=latch<0;
    wchar_t name[160];
    swprintf_s(name,L"Local\\DnaCalcProbe-%lu-%lld",GetCurrentProcessId(),(long long)(latch<0?-latch:latch));
    p->release=CreateEventW(nullptr,TRUE,FALSE,name);
    AcquireSRWLockExclusive(&pendingLock); pending.push_back(p); ReleaseSRWLockExclusive(&pendingLock);
    log(20,p->site,(int)latch,handle,(uintptr_t)handle->value.binary.bytes);
    auto copied=num(value); log(25,p->site,0,&copied,(uintptr_t)handle->value.binary.bytes);
    wchar_t entered[160];
    swprintf_s(entered,L"Local\\DnaCalcProbeEntered-%lu-%lld",GetCurrentProcessId(),(long long)(latch<0?-latch:latch));
    HANDLE signal=CreateEventW(nullptr,TRUE,TRUE,entered);
    if(signal)entrySignals.push_back(signal);
    activeCallbacks++;
    HANDLE t=CreateThread(nullptr,0,completeTracked,p,0,nullptr);
    if(t)CloseHandle(t);else {activeCallbacks--;log(29,p->site,(int)GetLastError());}
    log(21,p->site,0);
}
static DWORD WINAPI releaseLater(void* arg) { Sleep((DWORD)(uintptr_t)arg); ReleaseCalls(0); return 0; }
extern "C" __declspec(dllexport) int WINAPI ReleaseAfter(int milliseconds) {
    HANDLE t=CreateThread(nullptr,0,releaseLater,(void*)(uintptr_t)milliseconds,0,nullptr);
    if(t)CloseHandle(t); return t?1:0;
}
struct LatchTimer { long long latch; DWORD delay; };
static DWORD WINAPI releaseLatchLater(void* arg) {
    auto timer=(LatchTimer*)arg;
    wchar_t name[160]; swprintf_s(name,L"Local\\DnaCalcProbe-%lu-%lld",GetCurrentProcessId(),timer->latch);
    HANDLE event=CreateEventW(nullptr,TRUE,FALSE,name);
    Sleep(timer->delay); if(event){SetEvent(event);CloseHandle(event);} delete timer; return 0;
}
extern "C" __declspec(dllexport) int WINAPI ReleaseLatchAfter(int latch,int milliseconds) {
    auto timer=new LatchTimer{latch,(DWORD)milliseconds};
    HANDLE t=CreateThread(nullptr,0,releaseLatchLater,timer,0,nullptr);
    if(t)CloseHandle(t);else delete timer; return t?1:0;
}
static std::atomic<int> nextUiLatch{1000000000};
extern "C" __declspec(dllexport) void WINAPI AsyncUI(double value,double delay,Operand* handle) {
    int latch=++nextUiLatch;
    AsyncProbe(value,latch,handle);
    ReleaseLatchAfter(latch,(int)delay);
}
static Operand asyncRegistration{};
static bool registrationReady=false;
static int registrationGeneration=0;
extern "C" __declspec(dllexport) void WINAPI ProbeCalculationEnded() { log(50,0,0); }
extern "C" __declspec(dllexport) void WINAPI ProbeCalculationCanceled() { log(51,0,0); }
extern "C" __declspec(dllexport) int WINAPI UnregisterAsync() {
    if(!registrationReady){log(31,registrationGeneration,32);return 32;}
    log(30,registrationGeneration,0,&asyncRegistration);
    Operand answer{};
    int rc=call(201,&answer,{&asyncRegistration});log(31,registrationGeneration,rc,&answer);
    bool ok=rc==0 && (answer.type&0xfff)==4 && answer.value.integer!=0;
    if(ok)registrationReady=false;
    return ok?0:rc?rc:32;
}
extern "C" __declspec(dllexport) int WINAPI RegisterAsync() {
    Operand name{},id{},macro=integer(1);
    int rc=call(nameFn,&name);if(rc!=0){log(32,registrationGeneration,rc);return rc;}
    Text procedure(L"AsyncProbe"),types(L">BBX"),formula(L"AsyncEcho"),args(L"input,latch");
    rc=call(registerFn,&id,{&name,&procedure.value,&types.value,&formula.value,&args.value,&macro});
    bool ok=rc==0 && ((id.type&0xfff)==1 || (id.type&0xfff)==0x800);
    if(ok){asyncRegistration=id;registrationReady=true;registrationGeneration++;}
    log(32,registrationGeneration,rc,&id);
    call(freeFn,nullptr,{&name});
    return ok?0:rc?rc:32;
}
extern "C" __declspec(dllexport) int WINAPI xlAutoOpen() {
    if(!excel)excel=(Callback)GetProcAddress(GetModuleHandleW(nullptr),"MdCallBack12");
    if(!excel)return 0;
    reg(L"XBefore",L"BB",L"XBefore"); reg(L"XAfter",L"BB",L"XAfter");
    reg(L"AsyncUI",L">BBX",L"AsyncUI");
    // https://learn.microsoft.com/office/client-developer/excel/xleventregister
    Text ended(L"ProbeCalculationEnded"), canceled(L"ProbeCalculationCanceled");
    auto e1=integer(1), e2=integer(2); Operand answer{};
    int rc=call(0x4011,&answer,{&ended.value,&e1});log(52,1,rc,&answer);
    rc=call(0x4011,&answer,{&canceled.value,&e2});log(52,2,rc,&answer);
    return RegisterAsync()==0?1:0;
}
