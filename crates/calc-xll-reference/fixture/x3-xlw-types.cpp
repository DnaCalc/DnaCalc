// SPDX-License-Identifier: MIT
#include "x3-xlw-types.h"

double X3XlwAdd(double a, double b) { return a + b; }

NEMatrix X3XlwMatrix(NEMatrix values) {
    for (size_t row = 0; row < MatrixTraits<NEMatrix>::rows(values); ++row)
        for (size_t col = 0; col < MatrixTraits<NEMatrix>::columns(values); ++col)
            MatrixTraits<NEMatrix>::setAt(values, row, col,
                2 * MatrixTraits<NEMatrix>::getAt(values, row, col));
    return values;
}

double X3XlwReference(reftest value) {
    XlfOper operand(value);
    return operand.AsDouble();
}
