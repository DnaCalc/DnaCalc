# High-precision Gamma for binary64 arguments (Python decimal; Stirling at x+N, recurrence down).
from decimal import Decimal as D, getcontext
from fractions import Fraction as Fr
import math
getcontext().prec = 50
# Bernoulli numbers B2..B40 as fractions
def bernoulli(n):
    A = [Fr(0)] * (n + 1); B = []
    for m in range(n + 1):
        A[m] = Fr(1, m + 1)
        for j in range(m, 0, -1): A[j - 1] = j * (A[j - 1] - A[j])
        B.append(A[0])
    return B
_B = bernoulli(40)
PI = D("3.14159265358979323846264338327950288419716939937510582097494")
LN2PI_HALF = (2 * PI).ln() / 2
def lgamma_hp(x):
    """ln Gamma(x) for a positive binary64 x (as float), to ~45 digits"""
    xd = D(Fr(x).numerator) / D(Fr(x).denominator)
    N = 30
    z = xd + N
    s = (z - D("0.5")) * z.ln() - z + LN2PI_HALF
    zp = z; z2 = z * z
    for k in range(1, 20):
        b = _B[2 * k]
        s += D(b.numerator) / D(b.denominator) / (2 * k * (2 * k - 1)) / zp
        zp *= z2
    prod = D(1)
    for k in range(N): prod *= (xd + k)
    return s - prod.ln()
def gamma_hp(x):
    return lgamma_hp(x).exp()
def to_float_ulps(hp, v):
    """(v - hp) in ulps of v"""
    u = math.ulp(v)
    return float((D(Fr(v).numerator) / D(Fr(v).denominator) - hp) / D(Fr(u).numerator) * D(Fr(u).denominator))
if __name__ == "__main__":
    print(gamma_hp(1.5), math.gamma(1.5))
    print(lgamma_hp(2.5), math.lgamma(2.5))
