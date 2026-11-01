// SPDX-License-Identifier: MIT
// Fixture declarations consumed by the original xlw InterfaceGenerator.
#include <xlw/xlw.h>
#include <xlw/MyContainers.h>
using namespace xlw;
#define reftest LPXLFOPER
//<xlw:libraryname=X3Types

double // Adds two doubles through the native argument ABI.
//<xlw:threadsafe
X3XlwAdd(double a, double b);

NEMatrix // Doubles a numeric matrix through FP12 argument conversion.
//<xlw:threadsafe
X3XlwMatrix(NEMatrix values);

double // Reads a reference through xlw's own coercion path.
//<xlw:macrosheet
X3XlwReference(reftest value);
