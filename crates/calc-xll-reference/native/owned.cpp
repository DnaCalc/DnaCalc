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
    freeResult(registration);
    return rc==0&&registration.type==1;
}
extern "C" __declspec(dllexport) int __stdcall xlAutoClose(){
    if(!excel)return 1;
    Oper result;Oper* id[]={&registration};excel(201,1,id,&result);freeResult(result);
    auto name=counted(L"X0.NativeOwned");Oper text=textOper(name);Oper* args[]={&text};
    excel(88,1,args,&result);freeResult(result);
    return 1;
}
BOOL WINAPI DllMain(HINSTANCE instance,DWORD reason,LPVOID){
    if(reason==DLL_PROCESS_ATTACH){module=instance;DisableThreadLibraryCalls(instance);}return TRUE;
}
