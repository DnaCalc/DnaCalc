/* SPDX-License-Identifier: MIT. Original varargs adapter; no SDK header. */
#include <stdarg.h>
#include <stddef.h>
extern int __stdcall Excel12v(int, void*, int, void**);

__declspec(dllexport) int __cdecl Excel12(int function, void* result, int count, ...) {
    void* args[255];
    va_list list;
    int i;
    if (count < 0 || count > 255) return 4;
    va_start(list, count);
    for (i = 0; i < count; ++i) args[i] = va_arg(list, void*);
    va_end(list);
    return Excel12v(function, result, count, args);
}
/* Legacy XLOPER is a different layout. Refuse it until X7 implements that ABI. */
__declspec(dllexport) int __cdecl Excel4(int function, void* result, int count, ...) {
    (void)function; (void)result; (void)count; return 32;
}
__declspec(dllexport) int __stdcall Excel4v(int function, void* result, int count, void** args) {
    (void)function; (void)result; (void)count; (void)args; return 32;
}
