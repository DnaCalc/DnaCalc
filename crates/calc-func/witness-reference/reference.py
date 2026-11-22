"""High-precision reference values for the mismatching full-bank rows (dc-dnbcq).

Usage: python3 reference.py <judge-trace.jsonl> <out-dir> [FUNC ...]
Needs mpmath (pip3 install --target <dir> mpmath; PYTHONPATH=<dir>).  Reads a
`cargo xtask witness judge --bank full --trace` file, takes every row where
expected != actual, computes the true value with mpmath (60 digits) for the
functions in R, and writes one TSV per function (columns in README.md).
"""
import argparse
import collections
import json
import math
import os
import re
import signal
import struct
from concurrent.futures import ProcessPoolExecutor
from decimal import Decimal
from functools import lru_cache

import mpmath as mp
from mpmath.libmp import NoConvergence
mp.mp.dps = 60
mpf = mp.mpf


def f2b(x): return struct.unpack('<Q', struct.pack('<d', x))[0]
def b2f(b): return struct.unpack('<d', struct.pack('<Q', b))[0]
def isnum(s): return isinstance(s, str) and re.fullmatch(r'0x[0-9a-fA-F]{16}', s) is not None


def parse_arg(a, array_cell=False):
    if isinstance(a, bool): return a
    if isinstance(a, list): return [parse_arg(x, array_cell=True) for x in a]
    if isinstance(a, (int, float)): return float(a)
    if isnum(a): return b2f(int(a, 16))
    if isinstance(a, str) and not array_cell:
        try: return float(a)
        except ValueError: return a
    return a


def ulps_from(true, x):
    """Distance from the true value, using its binade or the subnormal spacing."""
    _, exponent = mp.frexp(true)
    spacing = mp.ldexp(1, max(-1074, exponent - 53) if true else -1074)
    return abs(mpf(x) - true) / spacing


def numeric_class(true, excel, ours):
    distances = ulps_from(true, excel), ulps_from(true, ours)
    if max(distances) > 16 or not math.isfinite(float(true)):
        return 'large-deviation', '', distances
    if true == 0 and excel == ours == 0:
        return 'structural:signed-zero', '', distances
    if abs(true) < mp.ldexp(1, -1022):
        rounded = int(mp.nint(mp.ldexp(abs(true), 1074)))
        if true < 0:
            rounded |= 1 << 63
    else:
        rounded = f2b(float(true))
    if f2b(excel) == rounded:
        return 'ours-wrong', '', distances
    detail = '' if f2b(ours) == rounded else 'both-inexact'
    return 'excel-quirk', detail, distances


R = {}
def ref(*names):
    def d(f):
        for n in names: R[n] = f
        return f
    return d


def fl(v): return int(mp.floor(mpf(v)))

# ---- elementary ----
@ref('ATAN')
def _(x): return mp.atan(mpf(x))
@ref('ATAN2')
def _(x, y): return mp.atan2(mpf(y), mpf(x))   # ATAN2(x_num, y_num)
@ref('ASIN')
def _(x): return mp.asin(mpf(x))
@ref('ACOS')
def _(x): return mp.acos(mpf(x))
@ref('COS')
def _(x): return mp.cos(mpf(x))
@ref('SIN')
def _(x): return mp.sin(mpf(x))
@ref('TAN')
def _(x): return mp.tan(mpf(x))
@ref('SINH')
def _(x): return mp.sinh(mpf(x))
@ref('COSH')
def _(x): return mp.cosh(mpf(x))
@ref('TANH')
def _(x): return mp.tanh(mpf(x))
@ref('COTH')
def _(x): return 1 / mp.tanh(mpf(x))
@ref('CSCH')
def _(x): return 1 / mp.sinh(mpf(x))
@ref('SECH')
def _(x): return 1 / mp.cosh(mpf(x))
@ref('ACOTH')
def _(x): return mp.acoth(mpf(x))
@ref('ASINH')
def _(x): return mp.asinh(mpf(x))
@ref('ACOSH')
def _(x): return mp.acosh(mpf(x))
@ref('ATANH')
def _(x): return mp.atanh(mpf(x))
@ref('EXP')
def _(x): return mp.exp(mpf(x))
@ref('LN')
def _(x): return mp.log(mpf(x))
@ref('LOG10')
def _(x): return mp.log10(mpf(x))
@ref('SQRT')
def _(x): return mp.sqrt(mpf(x))
@ref('GAMMA')
def _(x): return mp.gamma(mpf(x))
@ref('GAMMALN', 'GAMMALN.PRECISE')
def _(x): return mp.loggamma(mpf(x))

# ---- distributions ----
@ref('EXPON.DIST', 'EXPONDIST')
def _(x, lam, cum):
    x, lam = mpf(x), mpf(lam)
    return -mp.expm1(-lam * x) if cum else lam * mp.exp(-lam * x)

@ref('WEIBULL.DIST', 'WEIBULL')
def _(x, a, b, cum):
    x, a, b = mpf(x), mpf(a), mpf(b)
    if cum: return -mp.expm1(-(x / b) ** a)
    return a / b * (x / b) ** (a - 1) * mp.exp(-(x / b) ** a)

def _phi(z): return mp.erfc(-z / mp.sqrt(2)) / 2

@ref('NORM.S.DIST')
def _(z, cum):
    z = mpf(z)
    return _phi(z) if cum else mp.exp(-z * z / 2) / mp.sqrt(2 * mp.pi)
@ref('NORMSDIST')
def _(z): return _phi(mpf(z))
@ref('NORM.DIST', 'NORMDIST')
def _(x, m, s, cum):
    x, m, s = mpf(x), mpf(m), mpf(s)
    z = (x - m) / s
    return _phi(z) if cum else mp.exp(-z * z / 2) / (s * mp.sqrt(2 * mp.pi))


def gamma_probability(shape, x, upper=False):
    if shape <= 0 or x < 0:
        raise ValueError('gamma domain')
    if x == 0:
        return mpf(upper)
    if shape < 10**7 or abs(x - shape) > 40 * mp.sqrt(shape):
        if upper:
            return mp.gammainc(shape, x, mp.inf, regularized=True)
        return mp.gammainc(shape, 0, x, regularized=True)

    # Integrate the density around its mode in standardized coordinates for
    # large shapes, where the defining incomplete-gamma series is very slow.
    # The tangent to the log-concave density bounds the discarded tail.
    digits = mp.mp.dps
    with mp.workdps(digits + int(mp.log10(shape)) + 12):
        mode = mpf(shape) - 1
        sigma = mp.sqrt(mode)
        position = (x - mode) / sigma
        side = -1 if position < 0 else 1
        cutoff = -(digits + 8) * mp.log(10)
        step = 1 / max(1, abs(position / (1 + position / sigma)))
        panels = [mpf(0), step]

        def log_relative(offset):
            z = (position + side * offset) / sigma
            return mode * (mp.log1p(z) - z)

        base = log_relative(0)
        while log_relative(panels[-1]) - base > cutoff:
            panels.append(2 * panels[-1])
        area, error = mp.quad(lambda offset: mp.exp(log_relative(offset) - base),
                              panels, error=True)
        if error > abs(area) * mpf('1e-42'):
            raise ValueError('gamma density quadrature did not settle')
        normalizer = (mode + mpf('0.5')) * mp.log(mode) - mode - mp.loggamma(shape)
        tail = area * mp.exp(base + normalizer)
        return tail if upper == (side == 1) else 1 - tail


@ref('POISSON.DIST', 'POISSON')
def _(k, lam, cum):
    k = fl(k); lam = mpf(lam)
    if not cum:
        return mp.exp(-lam + k * mp.log(lam) - mp.loggamma(k + 1)) if lam > 0 else mpf(1 if k == 0 else 0)
    return gamma_probability(k + 1, lam, upper=True)

@ref('BINOM.DIST', 'BINOMDIST')
def _(k, n, p, cum):
    k = fl(k); n = fl(n); p = mpf(p)
    if not cum: return mp.binomial(n, k) * p ** k * (1 - p) ** (n - k)
    if k >= n: return mpf(1)
    return beta_lower(n - k, k + 1, 1 - p, p)

@ref('GAMMA.DIST', 'GAMMADIST')
def _(x, a, b, cum):
    x, a, b = mpf(x), mpf(a), mpf(b)
    if cum: return gamma_probability(a, x / b)
    if x == 0 and a == 1: return 1 / b
    return mp.exp((a - 1) * mp.log(x) - x / b - mp.loggamma(a) - a * mp.log(b))

@ref('CHISQ.DIST')
def _(x, df, cum):
    x, df = mpf(x), mpf(fl(df))
    if cum: return gamma_probability(df / 2, x / 2)
    if x == 0 and df == 2: return mpf('0.5')
    return mp.exp((df / 2 - 1) * mp.log(x) - x / 2 - mp.loggamma(df / 2) - df / 2 * mp.log(2))

@ref('CHISQ.DIST.RT', 'CHIDIST')
def _(x, df): return gamma_probability(mpf(fl(df)) / 2, mpf(x) / 2, upper=True)


@lru_cache(maxsize=512)
def log_beta(a, b, precision):
    # The normalizer is a difference of log-gamma values. Preserve the working
    # digits when a large shape makes the intermediate values much larger.
    extra = max(0, int(mp.log10(max(1, a, b)))) + 10
    with mp.workdps(precision + extra):
        return mp.loggamma(a) + mp.loggamma(b) - mp.loggamma(a + b)


def beta_lower(a, b, x, complement=None):
    """Regularized incomplete beta, DLMF 8.17.4 and 8.17.22–23."""
    a, b, x = map(mpf, (a, b, x))
    y = 1 - x if complement is None else mpf(complement)
    if x == 0: return mpf(0)
    if y == 0: return mpf(1)
    if a <= 0 or b <= 0 or x < 0 or y < 0:
        raise ValueError('beta domain')
    if a == b and x == y: return mpf('0.5')
    if a == b == 1: return x
    reflect = x * (b + 1) > y * (a + 1)
    # Reflection can leave a CDF of order b even when the evaluated tail is
    # close to one. Preserve the small shape before taking the complement.
    extra = max(0, -int(mp.floor(mp.log10(b)))) if reflect else 0
    if reflect:
        a, b, x, y = b, a, y, x

    with mp.extradps(extra + 8):
        log_tail = beta_log_tail(a, b, x, y)
        return -mp.expm1(log_tail) if reflect else mp.exp(log_tail)


def beta_log_tail(a, b, x, y):
    # Modified Lentz recurrence for the denominator of 8.17.22.
    tiny = mp.ldexp(1, -4 * mp.mp.prec)
    product = c = mpf(1)
    d = mpf(0)
    for m in range(5000):
        odd = -(a + m) * (a + b + m) * x / ((a + 2*m) * (a + 2*m + 1))
        even = (m + 1) * (b - m - 1) * x / ((a + 2*m + 1) * (a + 2*m + 2))
        for coefficient in (odd, even):
            d = 1 / (1 + coefficient * d or tiny)
            c = 1 + coefficient / c or tiny
            factor = c * d
            product *= factor
        if abs(factor - 1) < 16 * mp.eps:
            break
    else:
        raise ValueError('incomplete beta fraction did not converge')
    log_x = mp.log(x) if x < y else mp.log1p(-y)
    log_y = mp.log(y) if y < x else mp.log1p(-x)
    return a * log_x + b * log_y - log_beta(a, b, mp.mp.dps) - mp.log(a * product)


@ref('BETA.DIST')
def _(x, a, b, cum, lo=0.0, hi=1.0):
    x, a, b, lo, hi = map(mpf, (x, a, b, lo, hi))
    z, other = (x - lo) / (hi - lo), (hi - x) / (hi - lo)
    if cum: return beta_lower(a, b, z, other)
    return z ** (a - 1) * other ** (b - 1) / mp.beta(a, b) / (hi - lo)

@ref('BETADIST')
def _(x, a, b, lo=0.0, hi=1.0):
    x, a, b, lo, hi = map(mpf, (x, a, b, lo, hi))
    return beta_lower(a, b, (x - lo) / (hi - lo), (hi - x) / (hi - lo))

@ref('HYPGEOM.DIST')
def _(k, n, K, N, cum):
    k, n, K, N = fl(k), fl(n), fl(K), fl(N)
    pm = lambda i: mp.binomial(K, i) * mp.binomial(N - K, n - i) / mp.binomial(N, n)
    return sum(pm(i) for i in range(max(0, n - (N - K)), k + 1)) if cum else pm(k)

@ref('BESSELJ')
def _(x, n): return mp.besselj(fl(n), mpf(x))
@ref('BESSELY')
def _(x, n): return mp.bessely(fl(n), mpf(x))
@ref('BESSELI')
def _(x, n): return mp.besseli(fl(n), mpf(x))
@ref('BESSELK')
def _(x, n): return mp.besselk(fl(n), mpf(x))

# ---- financial ----
@ref('PMT')
def _(rate, nper, pv, fv=0.0, typ=0.0):
    return _pmt(rate, nper, pv, fv, int(bool(typ)))

@ref('FV')
def _(rate, nper, pmt, pv=0.0, typ=0.0):
    return _fv(rate, nper, pmt, pv, int(bool(typ)))

@ref('PV')
def _(rate, nper, pmt, fv=0.0, typ=0.0):
    rate, nper, pmt, fv = map(mpf, (rate, nper, pmt, fv)); t = 1 if typ else 0
    if rate == 0: return -(fv + pmt * nper)
    g = (1 + rate) ** nper
    return -(fv + pmt * (1 + rate * t) * (g - 1) / rate) / g

# ---- decimal rounding of the exact binary value ----
def _rdec(x, digits, mode):
    from fractions import Fraction
    if digits >= 1074 or x == 0:
        return mpf(x)
    if digits < -10000:
        if mode == 'up':
            return mp.sign(x) * mp.power(10, -digits)
        return mpf(0)
    s = Fraction(10) ** int(digits)
    v = Fraction(x) * s
    sign = -1 if v < 0 else 1
    a = abs(v)
    f = a.numerator // a.denominator
    if mode == 'down': r = f
    elif mode == 'up': r = f + (0 if a == f else 1)
    else: r = f + (1 if (a - f) >= Fraction(1, 2) else 0)
    q = Fraction(sign * r) / s
    return mpf(q.numerator) / mpf(q.denominator)

def _digits(d): return int(math.trunc(d))
@ref('ROUND')
def _(x, d): return _rdec(x, _digits(d), 'away')
@ref('ROUNDUP')
def _(x, d): return _rdec(x, _digits(d), 'up')
@ref('ROUNDDOWN', 'TRUNC')
def _(x, d=0.0): return _rdec(x, _digits(d), 'down')


X0 = 0.0
ROW_SECONDS = 10

class RowTimeout(Exception): pass
class NoRealReference(Exception): pass


def _alarm(*a): raise RowTimeout()
signal.signal(signal.SIGALRM, _alarm)

def illinois(g, a, b):
    """root of an increasing g on [a, b] with g(a) <= 0 <= g(b): Illinois regula falsi, stop at 1e-45 relative"""
    fa, fb = g(a), g(b); side = 0
    if fa == 0: return a
    if fb == 0: return b
    for _ in range(2000):
        c = (a * fb - b * fa) / (fb - fa)
        if not (min(a, b) < c < max(a, b)): c = (a + b) / 2
        fc = g(c)
        if fc == 0 or abs(b - a) <= mpf(10) ** -45 * abs(c): return c
        if fc < 0:
            a, fa = c, fc
            if side == -1: fb /= 2
            side = -1
        else:
            b, fb = c, fc
            if side == 1: fa /= 2
            side = 1
    raise ValueError('inverse did not converge')


def inv(cdf, p, positive=True, incr=True, x0=None, probability=True):
    """root of cdf(x) = p (cdf increasing, or decreasing with incr=False): bracket around Excel's answer
    (X0), then the Illinois method at working precision"""
    p = mpf(p); sgn = 1 if incr else -1
    if probability and not 0 < p < 1:
        raise ValueError('inverse probability outside the open interval')
    g = lambda x: sgn * (cdf(x) - p)
    if x0 is None: x0 = mpf(X0) if math.isfinite(X0) and X0 != 0 else mpf(1)
    # Most witness answers are already close to the root. Use a few secant
    # steps there before paying for a wider bracket.
    first = abs(x0) if positive else x0
    step = abs(first) if positive else max(1, abs(first))
    second = first + step * mpf('1e-6')
    f1, f2 = g(first), g(second)
    for _ in range(12):
        if f2 == 0:
            return second
        if f1 == f2:
            break
        candidate = second - f2 * (second - first) / (f2 - f1)
        if (positive and candidate <= 0) or abs(candidate - second) > step / 2:
            break
        if abs(candidate - second) <= step * mpf('1e-45') and abs(f2) <= p * mpf('1e-45'):
            return second
        first, f1, second = second, f2, candidate
        f2 = g(second)

    if positive:
        x0 = abs(x0); a, b = x0 / mpf('1.01'), x0 * mpf('1.01')
        for _ in range(4000):
            if g(a) <= 0: break
            a /= mpf('1.3')
        for _ in range(4000):
            if g(b) >= 0: break
            b *= mpf('1.3')
    else:
        d = max(abs(x0) * mpf('1e-3'), mpf('1e-3')); a, b = x0 - d, x0 + d
        for _ in range(200):
            if g(a) <= 0: break
            a -= d; d *= 2
        d = max(abs(x0) * mpf('1e-3'), mpf('1e-3'))
        for _ in range(200):
            if g(b) >= 0: break
            b += d; d *= 2
    if g(a) > 0 or g(b) < 0: raise ValueError('no bracket')
    return illinois(g, a, b)

def _q(p, x0=None):
    p = mpf(p)
    if p == mpf('0.5'):
        return mpf(0)
    if p > mpf('0.5'):
        return -_q(1 - p, -x0 if x0 is not None else -mpf(X0))
    return inv(_phi, p, positive=False, x0=x0)
@ref('NORM.S.INV', 'NORMSINV')
def _(p): return _q(p)
@ref('NORM.INV', 'NORMINV')
def _(p, m, s): return mpf(m) + mpf(s) * _q(p, (mpf(X0) - mpf(m)) / mpf(s))
@ref('LOGNORM.INV', 'LOGINV')
def _(p, m, s): return mp.exp(mpf(m) + mpf(s) * _q(p, (mp.log(X0) - mpf(m)) / mpf(s) if X0 > 0 else None))
@ref('CONFIDENCE.NORM', 'CONFIDENCE')
def _(a, sd, n): return -_q(mpf(a) / 2) * mpf(sd) / mp.sqrt(fl(n))
@ref('GAMMA.INV', 'GAMMAINV')
def _(p, a, b):
    return gamma_quantile(p, a, b)


def gamma_quantile(p, shape, scale, upper=False):
    p, shape, scale = map(mpf, (p, shape, scale))
    if p == int(upper): return mpf(0)
    if p > mpf('0.5'):
        p, upper = 1 - p, not upper
    if shape < 10**7:
        return inv(lambda x: gamma_probability(shape, x / scale, upper), p, incr=not upper)

    with mp.extradps(max(0, int(mp.log10(shape)) // 2 - 10)):
        sigma = mp.sqrt(shape)
        seed = (mpf(X0) / scale - shape) / sigma
        if abs(seed) > 40:
            seed = -_q(p, x0=mpf(-1)) if upper else _q(p, x0=mpf(-1))
        root = inv(lambda x: gamma_probability(shape, shape + x * sigma, upper),
                   p, positive=False, incr=not upper, x0=seed)
        return scale * (shape + root * sigma)


@ref('CHISQ.INV')
def _(p, df): return gamma_quantile(p, mpf(fl(df)) / 2, 2)
@ref('CHISQ.INV.RT', 'CHIINV')
def _(p, df): return gamma_quantile(p, mpf(fl(df)) / 2, 2, upper=True)
def _fcdf(x, d1, d2):
    d1, d2 = mpf(fl(d1)), mpf(fl(d2))
    z, y = d1 * x / (d1 * x + d2), d2 / (d1 * x + d2)
    return beta_lower(d1 / 2, d2 / 2, z, y)
def _fsf(x, d1, d2):
    d1, d2 = mpf(fl(d1)), mpf(fl(d2))
    z, y = d1 * x / (d1 * x + d2), d2 / (d1 * x + d2)
    return beta_lower(d2 / 2, d1 / 2, y, z)
@ref('F.INV')
def _(p, d1, d2):
    p = mpf(p)
    if p == 0: return mpf(0)
    if p > mpf('0.5'):
        return inv(lambda x: _fsf(x, d1, d2), 1 - p, incr=False)
    return inv(lambda x: _fcdf(x, d1, d2), p)
@ref('F.INV.RT', 'FINV')
def _(p, d1, d2):
    p = mpf(p)
    if p == 1: return mpf(0)
    if p > mpf('0.5'):
        return inv(lambda x: _fcdf(x, d1, d2), 1 - p)
    return inv(lambda x: _fsf(x, d1, d2), p, incr=False)
@ref('F.DIST')
def _(x, d1, d2, cum):
    x, a, b = mpf(x), mpf(fl(d1)), mpf(fl(d2))
    if cum: return _fcdf(x, a, b)
    if x == 0 and a >= 2: return mpf(a == 2)
    return mp.sqrt((a * x) ** a * b ** b / (a * x + b) ** (a + b)) / (x * mp.beta(a / 2, b / 2))
@ref('F.DIST.RT', 'FDIST')
def _(x, d1, d2): return _fsf(mpf(x), d1, d2)
def _tsf2(x, df):
    df = mpf(fl(df)); x = mpf(x)
    x2 = x * x
    return beta_lower(df / 2, mpf('0.5'), df / (df + x2), x2 / (df + x2))
def _tcdf(x, df):
    x = mpf(x); t = _tsf2(abs(x), df) / 2
    return 1 - t if x > 0 else t
@ref('T.DIST')
def _(x, df, cum):
    x, d = mpf(x), mpf(fl(df))
    if cum: return _tcdf(x, df)
    return mp.gamma((d + 1) / 2) / (mp.sqrt(d * mp.pi) * mp.gamma(d / 2)) * (1 + x * x / d) ** (-(d + 1) / 2)
@ref('T.DIST.RT')
def _(x, df): return _tsf2(abs(mpf(x)), df) / 2 if mpf(x) > 0 else 1 - _tsf2(abs(mpf(x)), df) / 2
@ref('T.DIST.2T')
def _(x, df): return _tsf2(abs(mpf(x)), df)
@ref('TDIST')
def _(x, df, tails): return _tsf2(abs(mpf(x)), df) / (1 if fl(tails) == 2 else 2)
@ref('T.INV')
def _(p, df): return inv(lambda x: _tcdf(x, df), p, positive=False)
@ref('T.INV.2T', 'TINV')
def _(p, df):
    if p == 1: return mpf(0)
    return inv(lambda x: _tsf2(x, df), p, incr=False)
@ref('CONFIDENCE.T')
def _(a, sd, n):
    n, a = fl(n), mpf(a)
    t = inv(lambda x: _tsf2(x, n - 1), a, incr=False, x0=mpf(1))
    return t * mpf(sd) / mp.sqrt(n)
@ref('BETA.INV', 'BETAINV')
def _(p, a, b, lo=0.0, hi=1.0):
    p, a, b, lo, hi = map(mpf, (p, a, b, lo, hi))
    if p == 0: return lo
    if p == 1: return hi
    if p == mpf('0.5') and a == b: return (lo + hi) / 2
    reflect = p > mpf('0.5')
    if reflect:
        p, a, b = 1 - p, b, a

    def tails(logit):
        ratio = mp.exp(-abs(logit))
        small = ratio / (1 + ratio)
        large = 1 / (1 + ratio)
        return (small, large) if logit < 0 else (large, small)

    def cdf(logit):
        z, other = tails(logit)
        return beta_lower(a, b, z, other)

    guess = mpf(X0)
    if lo < guess < hi:
        seed = mp.log((guess - lo) / (hi - guess))
        if reflect: seed = -seed
    else:
        seed = mp.log(a / b)
    logit = inv(cdf, p, positive=False, x0=seed)
    z, other = tails(logit)
    if reflect: z, other = other, z
    if z < other: return lo + z * (hi - lo)
    return hi - other * (hi - lo)
@ref('NEGBINOM.DIST')
def _(f, s, p, cum):
    f, s, p = fl(f), fl(s), mpf(p)
    pm = lambda i: mp.binomial(i + s - 1, i) * p ** s * (1 - p) ** i
    return beta_lower(s, f + 1, p) if cum else pm(f)
@ref('NEGBINOMDIST')
def _(f, s, p): return mp.binomial(fl(f) + fl(s) - 1, fl(f)) * mpf(p) ** fl(s) * (1 - mpf(p)) ** fl(f)
@ref('HYPGEOMDIST')
def _(k, n, K, N):
    k, n, K, N = fl(k), fl(n), fl(K), fl(N)
    return mp.binomial(K, k) * mp.binomial(N - K, n - k) / mp.binomial(N, n)
@ref('MULTINOMIAL')
def _(*xs):
    xs = [fl(x) for arg in xs for x in _flat(arg) if is_numeric_argument(x)]
    return mp.factorial(sum(xs)) / mp.fprod([mp.factorial(x) for x in xs])
@ref('ERFC')
def _(x): return mp.erfc(mpf(x))


def _flat(a):
    if not isinstance(a, list):
        yield a; return
    for x in a:
        if isinstance(x, list): yield from _flat(x)
        else: yield x


def is_numeric_argument(value):
    return type(value) is float

def _fv(r, n, pmt, pv, t):
    r, n, pmt, pv = map(mpf, (r, n, pmt, pv))
    if r == 0: return -(pv + pmt * n)
    if r > -1:
        exponent = n * mp.log1p(r)
        growth, growth_minus_one = mp.exp(exponent), mp.expm1(exponent)
    else:
        growth = (1 + r) ** n
        growth_minus_one = growth - 1
    return -(pv * growth + pmt * (1 + r * t) * growth_minus_one / r)
def _pmt(r, n, pv, fv, t):
    r, n, pv, fv = map(mpf, (r, n, pv, fv))
    if r == 0: return -(pv + fv) / n
    if r > -1:
        growth_minus_one = mp.expm1(n * mp.log1p(r))
    else:
        growth_minus_one = (1 + r) ** n - 1
    balance = mp.fsum([pv, fv, pv * growth_minus_one])
    return -balance * r / ((1 + r * t) * growth_minus_one)
def _ipmt(rate, per, nper, pv, fv=0.0, typ=0.0):
    t = 1 if typ else 0
    p = _pmt(rate, nper, pv, fv, t)
    if t and per == 1: return mpf(0)
    return _fv(rate, mpf(per) - (2 if t else 1), p, pv, t) * mpf(rate)


@ref('RATE')
def _(nper, pmt, pv, fv=0.0, typ=0.0, guess=0.1):
    nper, pmt, pv, fv = map(mpf, (nper, pmt, pv, fv))
    t = int(bool(typ))
    if pmt == 0 and pv != 0 and fv != 0 and nper != 0:
        if pv * fv > 0:
            raise NoRealReference('present and future cash flows have the same sign')
        return mp.expm1(mp.log(-fv / pv) / nper)
    if nper < 0 and t == 1 and pv * pmt < 0:
        # For negative periods, growing both terms to the future gives
        # same-sign contributions. At nonnegative rates their combined
        # magnitude is at least min(|pv|, |pmt|), and below zero it is
        # at least |pv|.
        if -mp.sign(pv) * fv < min(abs(pv), abs(pmt)):
            raise NoRealReference('future value is outside the annuity balance range')
    initial = mpf(X0 if math.isfinite(X0) else guess)
    if initial <= -1:
        raise ValueError('no real periodic rate in the worksheet domain')

    # Work in log(1 + rate) so iterates remain in the real annuity domain.
    def balance(q):
        return _fv(mp.expm1(q), nper, pmt, pv, t) - fv

    q0 = mp.log1p(initial)
    root = mp.findroot(balance, (q0, q0 + mpf('1e-7')), tol=mp.eps, maxsteps=100)
    scale = max(abs(mpf(pv)), abs(mpf(pmt) * nper), abs(fv))
    if abs(balance(root)) > scale * mpf('1e-45') or mp.diff(balance, root) == 0:
        raise ValueError('no verified simple rate root')
    return mp.expm1(root)


@ref('IPMT')
def _(rate, per, nper, pv, fv=0.0, typ=0.0): return _ipmt(rate, per, nper, pv, fv, typ)
@ref('PPMT')
def _(rate, per, nper, pv, fv=0.0, typ=0.0):
    return _pmt(rate, nper, pv, fv, 1 if typ else 0) - _ipmt(rate, per, nper, pv, fv, typ)
@ref('IRR')
def _(vals, guess=0.1):
    v = [mpf(x) for x in _flat(vals) if is_numeric_argument(x)]
    f = lambda r: sum(c / (1 + r) ** i for i, c in enumerate(v))
    x0 = mpf(X0)
    return mp.findroot(f, (x0, x0 * (1 + mpf(10) ** -10) + mpf(10) ** -12), solver='secant', tol=mpf(10) ** -45, maxsteps=200)
@ref('MIRR')
def _(vals, fin, rein):
    v = [mpf(x) for x in _flat(vals) if is_numeric_argument(x)]
    periods = len(v) - 1
    finance, reinvest = 1 + mpf(fin), 1 + mpf(rein)
    future = mp.fsum(c * reinvest ** (periods - i) for i, c in enumerate(v) if c > 0)
    present = mp.fsum(c / finance ** i for i, c in enumerate(v) if c < 0)
    ratio = -future / present
    if ratio < 0 and periods % 2 == 0:
        raise ValueError('no real MIRR root')
    return mp.sign(ratio) * mp.root(abs(ratio), periods) - 1
@ref('F.TEST', 'FTEST')
def _(a, b):
    a = [mpf(x) for x in _flat(a) if is_numeric_argument(x)]
    b = [mpf(x) for x in _flat(b) if is_numeric_argument(x)]
    def var(values):
        mean = mp.fsum(values) / len(values)
        return mp.fsum((x - mean) ** 2 for x in values) / (len(values) - 1)
    F = var(a) / var(b)
    d1, d2 = len(a) - 1, len(b) - 1
    return 2 * min(_fcdf(F, d1, d2), _fsf(F, d1, d2))
def _pairs(a, b):
    return [(mpf(x), mpf(y)) for x, y in zip(_flat(a), _flat(b), strict=True)
            if is_numeric_argument(x) and is_numeric_argument(y)]
@ref('SUMX2MY2')
def _(a, b): return mp.fsum(term for x, y in _pairs(a, b) for term in (x * x, -y * y))
@ref('SUMX2PY2')
def _(a, b): return mp.fsum(term for x, y in _pairs(a, b) for term in (x * x, y * y))
@ref('SUMXMY2')
def _(a, b): return mp.fsum((x - y) ** 2 for x, y in _pairs(a, b))


# ---- bonds (PRICE, YIELD, DURATION, MDURATION), 1900 date system ----
import datetime as _dt, calendar as _cal
def _date(serial):
    n = fl(serial)
    if n == 60:
        raise ValueError('fictitious 1900 leap day')
    epoch = _dt.date(1899, 12, 30) if n >= 61 else _dt.date(1899, 12, 31)
    return epoch + _dt.timedelta(days=n)
def _eom(d): return d.day == _cal.monthrange(d.year, d.month)[1]
def _addm(d, m, eom):
    y, mo = divmod(d.year * 12 + d.month - 1 + m, 12); mo += 1
    dim = _cal.monthrange(y, mo)[1]
    return _dt.date(y, mo, dim if eom else min(d.day, dim))
def _d360(a, b, eu):
    d1, d2 = a.day, b.day
    if eu: d1 = min(d1, 30); d2 = min(d2, 30)
    else:
        if a.month == 2 and _eom(a) and b.month == 2 and _eom(b): d2 = 30
        if a.month == 2 and _eom(a): d1 = 30
        if d2 == 31 and d1 >= 30: d2 = 30
        if d1 == 31: d1 = 30
    return (b.year - a.year) * 360 + (b.month - a.month) * 30 + d2 - d1
def _coupon(settle, mat, freq, basis):
    s, m = _date(settle), _date(mat); step = 12 // freq; eom = _eom(m)
    k = 0
    while _addm(m, -k * step, eom) > s: k += 1
    pcd = _addm(m, -k * step, eom); ncd = _addm(m, -(k - 1) * step, eom)
    N = k
    if basis == 0: A = _d360(pcd, s, False); E = mpf(360) / freq
    elif basis == 4: A = _d360(pcd, s, True); E = mpf(360) / freq
    elif basis == 1: A = (s - pcd).days; E = (ncd - pcd).days
    elif basis == 2: A = (s - pcd).days; E = mpf(360) / freq
    else: A = (s - pcd).days; E = mpf(365) / freq
    A, E = mpf(A), mpf(E)
    DSC = E - A
    return N, A, E, DSC
def _price(settle, mat, rate, yld, red, freq, basis):
    freq, basis = fl(freq), fl(basis)
    N, A, E, DSC = _coupon(settle, mat, freq, basis)
    rate, yld, red = mpf(rate), mpf(yld), mpf(red); c = 100 * rate / freq; y = yld / freq
    if N == 1: return (red + c) / (1 + DSC / E * y) - A / E * c
    return red / (1 + y) ** (N - 1 + DSC / E) + sum(c / (1 + y) ** (k - 1 + DSC / E) for k in range(1, N + 1)) - c * A / E
@ref('PRICE')
def _(settle, mat, rate, yld, red, freq, basis=0.0): return _price(settle, mat, rate, yld, red, freq, basis)
@ref('YIELD')
def _(settle, mat, rate, pr, red, freq, basis=0.0):
    pr = mpf(pr)
    return inv(lambda y: _price(settle, mat, rate, y, red, freq, basis), pr,
               positive=False, incr=False, probability=False)
def _dur(settle, mat, rate, yld, freq, basis):
    freq, basis = fl(freq), fl(basis)
    N, A, E, DSC = _coupon(settle, mat, freq, basis)
    rate, yld = mpf(rate), mpf(yld); c = 100 * rate / freq; y = yld / freq
    cfs = [(mpf(k - 1) + DSC / E, c + (100 if k == N else 0)) for k in range(1, N + 1)]
    pv = sum(cf / (1 + y) ** t for t, cf in cfs)
    return sum(t * cf / (1 + y) ** t for t, cf in cfs) / pv / freq, y
@ref('DURATION')
def _(settle, mat, rate, yld, freq, basis=0.0): return _dur(settle, mat, rate, yld, freq, basis)[0]
@ref('MDURATION')
def _(settle, mat, rate, yld, freq, basis=0.0):
    d, y = _dur(settle, mat, rate, yld, freq, basis); return d / (1 + y)


# Text answers carry real and imaginary components rounded to 15 significant digits.
_DECIMAL = r'(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?'
_COMPLEX = re.compile(
    rf'(?:(?P<real>[+-]?{_DECIMAL})(?=[+-]))?(?P<sign>[+-]?)(?P<imag>{_DECIMAL})?[ij]'
)


def complex_parts(text):
    if text.startswith('text:'):
        text = text[5:]
    match = _COMPLEX.fullmatch(text)
    if match is None:
        return Decimal(text), Decimal(0)
    return Decimal(match['real'] or '0'), Decimal(match['sign'] + (match['imag'] or '1'))


def complex_arg(value):
    if isinstance(value, str):
        return mp.mpc(*(mpf(str(part)) for part in complex_parts(value)))
    return mp.mpc(value)


TEXTREF = {
    'IMSINH': lambda x: mp.sinh(complex_arg(x)),
    'IMCOSH': lambda x: mp.cosh(complex_arg(x)),
    'IMCSCH': lambda x: 1 / mp.sinh(complex_arg(x)),
    'IMSECH': lambda x: 1 / mp.cosh(complex_arg(x)),
    'IMTAN': lambda x: mp.tan(complex_arg(x)),
    'IMCOT': lambda x: 1 / mp.tan(complex_arg(x)),
    'COMPLEX': lambda re, im, *_: mp.mpc(re, im),
}


def text_class(function, args, excel, ours):
    true = TEXTREF[function](*args)
    want = tuple(Decimal(mp.nstr(component, 15)) for component in (true.real, true.imag))
    correct_excel = complex_parts(excel) == want
    correct_ours = complex_parts(ours) == want
    if correct_excel and correct_ours:
        cls, detail = 'structural:text-format', 'text15'
    elif correct_excel:
        cls, detail = 'ours-wrong', 'text15'
    else:
        cls = 'excel-quirk'
        detail = 'text15' if correct_ours else 'text15,both-inexact'
    return cls, detail, mp.nstr(true, 25)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('trace')
    parser.add_argument('outdir')
    parser.add_argument('functions', nargs='*')
    parser.add_argument('--jobs', type=int, default=1)
    options = parser.parse_args()
    if options.jobs < 1:
        parser.error('--jobs must be positive')
    only = set(options.functions)
    os.makedirs(options.outdir, exist_ok=True)
    rows = collections.defaultdict(list)
    with open(options.trace, encoding='utf-8') as trace:
        for line in trace:
            r = json.loads(line)
            function = r['function']
            if is_match(r['expected'], r['actual']) or (only and function not in only):
                continue
            rows[function].append(r)
    batches = [(f, rs, options.outdir) for f, rs in rows.items()]
    if options.jobs == 1:
        for batch in batches:
            classify(*batch)
    else:
        # Prioritize the large per-function tables and keep each output in one process.
        batches.sort(key=lambda batch: -len(batch[1]))
        with ProcessPoolExecutor(max_workers=options.jobs) as workers:
            list(workers.map(classify_batch, batches))


def is_match(excel, ours):
    if excel == ours:
        return True
    for prefix in ('error:', 'logical:'):
        if excel.startswith(prefix) and ours.startswith(prefix):
            return excel.lower() == ours.lower()
    if isnum(excel) and isnum(ours):
        return int(excel, 16) == int(ours, 16)
    return False


def kind(v):
    if isnum(v): return 'number'
    if isinstance(v, str):
        if v.startswith('error:'): return v
        if v.startswith('fault:'): return 'refused'
        if v.startswith('excluded:'): return 'excluded'
        if v.startswith('text:'): return 'text'
        if v.startswith('logical:'): return 'logical'
    return 'other'


_OPTIONAL = {
    'BETA.DIST': {4: 0.0, 5: 1.0},
    **dict.fromkeys(('BETADIST', 'BETAINV', 'BETA.INV'), {3: 0.0, 4: 1.0}),
    **dict.fromkeys(('PRICE', 'YIELD'), {6: 0.0}),
    **dict.fromkeys(('DURATION', 'MDURATION'), {5: 0.0}),
    **dict.fromkeys(('IPMT', 'PPMT'), {4: 0.0, 5: 0.0}),
    'RATE': {3: 0.0, 4: 0.0, 5: 0.1},
}


def reference_args(function, raw_args, reading):
    defaults = _OPTIONAL.get(function, {})
    args = [defaults.get(i, 0.0) if a is None else parse_arg(a) for i, a in enumerate(raw_args)]
    if function in ('PRICE', 'YIELD', 'DURATION', 'MDURATION'):
        dates = reading.get('invocation', {}).get('date_system', '1900')
        if dates == '1904':
            args[0] = fl(args[0]) + 1462
            args[1] = fl(args[1]) + 1462
        elif dates != '1900':
            raise ValueError('unknown date system')
    return args


def reference_row(function, row):
    global X0
    excel, ours = row['expected'], row['actual']
    structural = f'structural:{kind(excel)}/{kind(ours)}'
    args = row['args']
    if isnum(excel) and not math.isfinite(b2f(int(excel, 16))):
        return '', '', '', 'structural:nonfinite-excel-result', ''
    if isnum(ours) and not math.isfinite(b2f(int(ours, 16))):
        return '', '', '', 'structural:nonfinite-dna-result', ''
    numbers = tuple(b2f(int(value, 16)) if isnum(value) else None for value in (excel, ours))
    both_numbers = all(value is not None for value in numbers)
    text = function in TEXTREF and excel.startswith('text:') and ours.startswith('text:')
    if not text and all(value is None for value in numbers):
        return '', '', '', structural, ''
    if not text and function not in R:
        return '', '', '', 'no-reference' if both_numbers else structural, ''
    try:
        args = reference_args(function, args, row.get('reading', {}))
        X0 = next((value for value in numbers if value is not None), 0.0)
        signal.alarm(ROW_SECONDS)
        try:
            if text:
                cls, detail, true = text_class(function, args, excel, ours)
                return true, '', '', cls, detail
            true = mpf(R[function](*args))
        finally:
            signal.alarm(0)
        if not mp.isfinite(true):
            raise ValueError('nonfinite reference')
        if both_numbers:
            cls, detail, distances = numeric_class(true, *numbers)
        else:
            cls, detail = structural, ''
            distances = [ulps_from(true, value) if value is not None else None for value in numbers]
        return (mp.nstr(true, 25),
                *(mp.nstr(d, 9) if d is not None else '' for d in distances), cls, detail)
    except NoRealReference as error:
        return '', '', '', 'structural:no-real-root' if both_numbers else structural, str(error)
    except (ArithmeticError, ValueError, TypeError, NoConvergence, RowTimeout) as error:
        failure = f'reference-failed:{type(error).__name__}'
        if not (text or both_numbers):
            return '', '', '', structural, failure
        return '', '', '', failure, '' if isinstance(error, RowTimeout) else str(error)


def escaped(text):
    """JSON-style escaping within a TSV field, without the enclosing quotes."""
    return json.dumps(text, ensure_ascii=True)[1:-1]


def classify_batch(batch):
    classify(*batch)


def classify(function, rows, outdir):
    counts = collections.Counter()
    with open(os.path.join(outdir, function + '.tsv'), 'w', encoding='utf-8', newline='\n') as out:
        out.write('id\targs\texcel\tours\treference\texcel_ulp\tours_ulp\tclass\tdetail\tsource\tresult_index\treading\n')
        for row in rows:
            true, ue, uo, cls, detail = reference_row(function, row)
            counts[cls] += 1
            fields = [
                escaped(row['id']), json.dumps(row['args']),
                escaped(row['expected']), escaped(row['actual']), true, ue, uo, cls,
                escaped(detail), escaped(row.get('source', '')),
                json.dumps(row.get('result_index')), json.dumps(row.get('reading', {}), sort_keys=True),
            ]
            out.write('\t'.join(fields) + '\n')
    print(function, len(rows), dict(counts), flush=True)


if __name__ == '__main__':
    main()
