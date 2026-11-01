#define xlAutoOpen BaseAutoOpen
#include "../P03/native-probe.cpp"
#undef xlAutoOpen
extern "C" __declspec(dllexport) double WINAPI XPure(double value) {
    auto v=num(value);log(40,0,0,&v);return value;
}
extern "C" __declspec(dllexport) int WINAPI ObserveNative(int type,double value,int site) {
    auto v=num(value);log(41,site,type,&v);return 1;
}
extern "C" __declspec(dllexport) int WINAPI xlAutoOpen() {
    if(!excel)excel=(Callback)GetProcAddress(GetModuleHandleW(nullptr),"MdCallBack12");
    if(!excel)return 0;
    reg(L"XPure",L"BB$",L"XPure");return 1;
}
