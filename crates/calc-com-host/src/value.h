// SPDX-License-Identifier: MIT
#pragma once
#include <cstddef>
// Owned BSTR only when returned by the RTD bridge; ordinary Run uses borrowed input.
struct Value { unsigned kind; double number; const wchar_t* text; size_t length; };
