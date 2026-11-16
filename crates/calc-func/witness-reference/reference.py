"""High-precision reference values for the mismatching full-bank rows (dc-dnbcq).

Usage: python3 reference.py <judge-trace.jsonl> <out-dir> [FUNC ...]
Needs mpmath (pip3 install --target <dir> mpmath; PYTHONPATH=<dir>).  Reads a
`cargo xtask witness judge --bank full --trace` file, takes every row where
expected != actual, computes the true value with mpmath (60 digits) for the
functions in R, and writes one TSV per function (columns in README.md).
"""
import json, sys, os, struct, collections, math, signal
import mpmath as mp
mp.mp.dps = 60
mpf = mp.mpf


def f2b(x): return struct.unpack('<Q', struct.pack('<d', x))[0]
def b2f(b): return struct.unpack('<d', struct.pack('<Q', b))[0]
def isnum(s): return isinstance(s, str) and s.startswith('0x') and len(s) == 18


def parse_arg(a):
    if isinstance(a, bool): return a
    if isinstance(a, list): return [parse_arg(x) for x in a]
    if isinstance(a, (int, float)): return float(a)
    if isnum(a): return b2f(int(a, 16))
    if isinstance(a, str):
        try: return float(a)
        except ValueError: return a
    return a


def ulps_from(true, x):
    """distance of double x from the true value, in ulps of the binade of the true value"""
    t = mpf(true)
    if t == 0: return None if x != 0 else mpf(0)
    ft = float(t)
    if ft == 0 or math.isinf(ft): return None
    m, e = math.frexp(abs(ft))
    u = mpf(2) ** (e - 53)
    if abs(ft) < 2.2250738585072014e-308: u = mpf(2) ** -1074
    return abs(mpf(x) - t) / u


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

@ref('POISSON.DIST', 'POISSON')
def _(k, lam, cum):
    k = fl(k); lam = mpf(lam)
    if not cum:
        return mp.exp(-lam + k * mp.log(lam) - mp.loggamma(k + 1)) if lam > 0 else mpf(1 if k == 0 else 0)
    return mp.gammainc(k + 1, lam, mp.inf, regularized=True)

@ref('BINOM.DIST', 'BINOMDIST')
def _(k, n, p, cum):
    k = fl(k); n = fl(n); p = mpf(p)
    if not cum: return mp.binomial(n, k) * p ** k * (1 - p) ** (n - k)
    if k >= n: return mpf(1)
    return mp.betainc(n - k, k + 1, 0, 1 - p, regularized=True)

@ref('GAMMA.DIST', 'GAMMADIST')
def _(x, a, b, cum):
    x, a, b = mpf(x), mpf(a), mpf(b)
    if cum: return mp.gammainc(a, 0, x / b, regularized=True)
    return mp.exp((a - 1) * mp.log(x) - x / b - mp.loggamma(a) - a * mp.log(b))

@ref('CHISQ.DIST')
def _(x, df, cum):
    x, df = mpf(x), mpf(fl(df))
    if cum: return mp.gammainc(df / 2, 0, x / 2, regularized=True)
    return mp.exp((df / 2 - 1) * mp.log(x) - x / 2 - mp.loggamma(df / 2) - df / 2 * mp.log(2))

@ref('CHISQ.DIST.RT', 'CHIDIST')
def _(x, df): return mp.gammainc(mpf(fl(df)) / 2, mpf(x) / 2, mp.inf, regularized=True)

@ref('BETA.DIST')
def _(x, a, b, cum, lo=0.0, hi=1.0):
    x, a, b, lo, hi = map(mpf, (x, a, b, lo, hi))
    z = (x - lo) / (hi - lo)
    if cum: return mp.betainc(a, b, 0, z, regularized=True)
    return z ** (a - 1) * (1 - z) ** (b - 1) / mp.beta(a, b) / (hi - lo)

@ref('BETADIST')
def _(x, a, b, lo=0.0, hi=1.0):
    x, a, b, lo, hi = map(mpf, (x, a, b, lo, hi))
    return mp.betainc(a, b, 0, (x - lo) / (hi - lo), regularized=True)

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
    rate, nper, pv, fv = map(mpf, (rate, nper, pv, fv))
    if rate == 0: return -(pv + fv) / nper
    g = (1 + rate) ** nper
    return -(pv * g + fv) * rate / ((1 + rate * (1 if typ else 0)) * (g - 1))

@ref('FV')
def _(rate, nper, pmt, pv=0.0, typ=0.0):
    rate, nper, pmt, pv = map(mpf, (rate, nper, pmt, pv)); t = 1 if typ else 0
    if rate == 0: return -(pv + pmt * nper)
    g = (1 + rate) ** nper
    return -(pv * g + pmt * (1 + rate * t) * (g - 1) / rate)

@ref('PV')
def _(rate, nper, pmt, fv=0.0, typ=0.0):
    rate, nper, pmt, fv = map(mpf, (rate, nper, pmt, fv)); t = 1 if typ else 0
    if rate == 0: return -(fv + pmt * nper)
    g = (1 + rate) ** nper
    return -(fv + pmt * (1 + rate * t) * (g - 1) / rate) / g

# ---- decimal rounding of the exact binary value ----
def _rdec(x, digits, mode):
    from fractions import Fraction
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
ROW_SECONDS = 4

class RowTimeout(Exception): pass
def _alarm(*a): raise RowTimeout()
signal.signal(signal.SIGALRM, _alarm)

def inv(cdf, p, positive=True, incr=True):
    """root of cdf(x) = p (cdf increasing, or decreasing with incr=False): bracket around Excel's answer
    (X0), then the Illinois method at working precision"""
    p = mpf(p); sgn = 1 if incr else -1
    g = lambda x: sgn * (cdf(x) - p)
    x0 = mpf(X0) if math.isfinite(X0) and X0 != 0 else mpf(1)
    if positive:
        x0 = abs(x0); a, b = x0 / 2, x0 * 2
        for _ in range(4000):
            if g(a) <= 0: break
            a /= 4
        for _ in range(4000):
            if g(b) >= 0: break
            b *= 4
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
    return mp.findroot(g, (a, b), solver='illinois', tol=mpf(10) ** -50, maxsteps=400, verify=False)

def _q(p): return inv(_phi, p, positive=False)   # normal quantile
@ref('NORM.S.INV', 'NORMSINV')
def _(p): return _q(p)
@ref('NORM.INV', 'NORMINV')
def _(p, m, s): return mpf(m) + mpf(s) * _q(p)
@ref('LOGNORM.INV', 'LOGINV')
def _(p, m, s): return mp.exp(mpf(m) + mpf(s) * _q(p))
@ref('CONFIDENCE.NORM', 'CONFIDENCE')
def _(a, sd, n): return -_q(mpf(a) / 2) * mpf(sd) / mp.sqrt(fl(n))
@ref('GAMMA.INV', 'GAMMAINV')
def _(p, a, b):
    a, b = mpf(a), mpf(b)
    return inv(lambda x: mp.gammainc(a, 0, x / b, regularized=True), p)
@ref('CHISQ.INV')
def _(p, df): return inv(lambda x: mp.gammainc(mpf(fl(df)) / 2, 0, x / 2, regularized=True), p)
@ref('CHISQ.INV.RT', 'CHIINV')
def _(p, df): return inv(lambda x: mp.gammainc(mpf(fl(df)) / 2, x / 2, mp.inf, regularized=True), p, incr=False)
def _fcdf(x, d1, d2):
    d1, d2 = mpf(fl(d1)), mpf(fl(d2))
    return mp.betainc(d1 / 2, d2 / 2, 0, d1 * x / (d1 * x + d2), regularized=True)
def _fsf(x, d1, d2):
    d1, d2 = mpf(fl(d1)), mpf(fl(d2))
    return mp.betainc(d2 / 2, d1 / 2, 0, d2 / (d1 * x + d2), regularized=True)
@ref('F.INV')
def _(p, d1, d2): return inv(lambda x: _fcdf(x, d1, d2), p)
@ref('F.INV.RT', 'FINV')
def _(p, d1, d2): return inv(lambda x: _fsf(x, d1, d2), p, incr=False)
@ref('F.DIST')
def _(x, d1, d2, cum):
    x, a, b = mpf(x), mpf(fl(d1)), mpf(fl(d2))
    if cum: return _fcdf(x, a, b)
    return mp.sqrt((a * x) ** a * b ** b / (a * x + b) ** (a + b)) / (x * mp.beta(a / 2, b / 2))
@ref('F.DIST.RT', 'FDIST')
def _(x, d1, d2): return _fsf(mpf(x), d1, d2)
def _tsf2(x, df):
    df = mpf(fl(df)); x = mpf(x)
    return mp.betainc(df / 2, mpf(1) / 2, 0, df / (df + x * x), regularized=True)
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
def _(p, df): return inv(lambda x: _tsf2(x, df), p, incr=False)
@ref('CONFIDENCE.T')
def _(a, sd, n):
    n = fl(n); a = mpf(a)
    global X0
    keep = X0; X0 = 1.0
    t = inv(lambda x: _tsf2(x, n - 1), a, incr=False)
    X0 = keep
    return t * mpf(sd) / mp.sqrt(n)
@ref('BETA.INV', 'BETAINV')
def _(p, a, b, lo=0.0, hi=1.0):
    a, b, lo, hi = map(mpf, (a, b, lo, hi))
    z = inv_unit(lambda z: mp.betainc(a, b, 0, z, regularized=True), p, (mpf(X0) - lo) / (hi - lo))
    return lo + z * (hi - lo)
def inv_unit(cdf, p, z0):
    p = mpf(p)
    if not (0 < z0 < 1): z0 = mpf('0.5')
    return mp.findroot(lambda z: cdf(z) - p, (z0, z0 * (1 + mpf(10) ** -12)), solver='secant', tol=mpf(10) ** -45, maxsteps=300)
@ref('NEGBINOM.DIST')
def _(f, s, p, cum):
    f, s, p = fl(f), fl(s), mpf(p)
    pm = lambda i: mp.binomial(i + s - 1, i) * p ** s * (1 - p) ** i
    return mp.betainc(s, f + 1, 0, p, regularized=True) if cum else pm(f)
@ref('NEGBINOMDIST')
def _(f, s, p): return mp.binomial(fl(f) + fl(s) - 1, fl(f)) * mpf(p) ** fl(s) * (1 - mpf(p)) ** fl(f)
@ref('HYPGEOMDIST')
def _(k, n, K, N):
    k, n, K, N = fl(k), fl(n), fl(K), fl(N)
    return mp.binomial(K, k) * mp.binomial(N - K, n - k) / mp.binomial(N, n)
@ref('MULTINOMIAL')
def _(*xs):
    xs = [fl(x) for x in xs]
    return mp.factorial(sum(xs)) / mp.fprod([mp.factorial(x) for x in xs])
@ref('ERFC')
def _(x): return mp.erfc(mpf(x))


def _flat(a):
    if not isinstance(a, list):
        yield a; return
    for x in a:
        if isinstance(x, list): yield from _flat(x)
        else: yield x

def _fv(r, n, pmt, pv, t):
    r, n, pmt, pv = map(mpf, (r, n, pmt, pv))
    if r == 0: return -(pv + pmt * n)
    g = (1 + r) ** n
    return -(pv * g + pmt * (1 + r * t) * (g - 1) / r)
def _pmt(r, n, pv, fv, t):
    r, n, pv, fv = map(mpf, (r, n, pv, fv))
    if r == 0: return -(pv + fv) / n
    g = (1 + r) ** n
    return -(pv * g + fv) * r / ((1 + r * t) * (g - 1))
def _ipmt(rate, per, nper, pv, fv=0.0, typ=0.0):
    t = 1 if typ else 0
    p = _pmt(rate, nper, pv, fv, t)
    if t and per == 1: return mpf(0)
    return _fv(rate, mpf(per) - (2 if t else 1), p, pv, t) * mpf(rate)
@ref('IPMT')
def _(rate, per, nper, pv, fv=0.0, typ=0.0): return _ipmt(rate, per, nper, pv, fv, typ)
@ref('PPMT')
def _(rate, per, nper, pv, fv=0.0, typ=0.0):
    return _pmt(rate, nper, pv, fv, 1 if typ else 0) - _ipmt(rate, per, nper, pv, fv, typ)
@ref('IRR')
def _(vals, guess=0.1):
    v = [mpf(x) for x in _flat(vals)]
    f = lambda r: sum(c / (1 + r) ** i for i, c in enumerate(v))
    x0 = mpf(X0)
    return mp.findroot(f, (x0, x0 * (1 + mpf(10) ** -10) + mpf(10) ** -12), solver='secant', tol=mpf(10) ** -45, maxsteps=200)
@ref('MIRR')
def _(vals, fin, rein):
    v = [mpf(x) for x in _flat(vals)]; n = len(v); fin, rein = mpf(fin), mpf(rein)
    pos = sum(c / (1 + rein) ** i for i, c in enumerate(v) if c > 0)
    neg = sum(c / (1 + fin) ** i for i, c in enumerate(v) if c < 0)
    return (-pos * (1 + rein) ** n / (neg * (1 + fin))) ** (mpf(1) / (n - 1)) - 1
@ref('F.TEST', 'FTEST')
def _(a, b):
    a = [mpf(x) for x in _flat(a)]; b = [mpf(x) for x in _flat(b)]
    var = lambda v: sum((x - sum(v) / len(v)) ** 2 for x in v) / (len(v) - 1)
    F = var(a) / var(b)
    d1, d2 = len(a) - 1, len(b) - 1
    c = _fcdf(F, d1, d2)
    return 2 * min(c, 1 - c)
def _pairs(a, b): return list(zip((mpf(x) for x in _flat(a)), (mpf(x) for x in _flat(b))))
@ref('SUMX2MY2')
def _(a, b): return sum(x * x - y * y for x, y in _pairs(a, b))
@ref('SUMX2PY2')
def _(a, b): return sum(x * x + y * y for x, y in _pairs(a, b))
@ref('SUMXMY2')
def _(a, b): return sum((x - y) ** 2 for x, y in _pairs(a, b))


# ---- bonds (PRICE, YIELD, DURATION, MDURATION), 1900 date system ----
import datetime as _dt, calendar as _cal
def _date(serial):
    n = int(math.floor(serial))
    return _dt.date(1899, 12, 30) + _dt.timedelta(days=n) if n >= 61 else _dt.date(1899, 12, 31) + _dt.timedelta(days=n - 1)
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
    return inv(lambda y: _price(settle, mat, rate, y, red, freq, basis), pr, positive=False, incr=False)
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


def main():
    trace, outdir = sys.argv[1], sys.argv[2]
    only = set(sys.argv[3:])
    os.makedirs(outdir, exist_ok=True)
    rows = collections.defaultdict(list)
    for line in open(trace):
        r = json.loads(line)
        if 'expected' not in r: continue
        f = r['function']
        if r['expected'] == r['actual']: continue
        if only and f not in only: continue
        rows[f].append(r)
    for f, rs in sorted(rows.items()):
        classify(f, rs, outdir)


def okey(x):
    b = f2b(x)
    return b if b < (1 << 63) else -(b - (1 << 63))


def kind(v):
    if isnum(v): return 'number'
    if isinstance(v, str):
        if v.startswith('error:'): return v
        if v.startswith('fault:'): return 'refused'
        return 'text'
    return 'other'


def classify(f, rs, outdir):
    out = open(os.path.join(outdir, f + '.tsv'), 'w')
    out.write('id\targs\texcel\tours\treference\texcel_ulp\tours_ulp\tclass\n')
    counts = collections.Counter(); worst_e = worst_o = 0
    for r in rs:
        e, a = r['expected'], r['actual']
        args = [parse_arg(x) for x in r['args']]
        cls = None; ref_s = ''; ue = uo = ''
        if not (isnum(e) and isnum(a)):
            cls = 'structural:%s/%s' % (kind(e), kind(a))
        elif f not in R:
            cls = 'no-reference'
            d = abs(okey(b2f(int(e, 16))) - okey(b2f(int(a, 16))))
            ue = ''; uo = ''; ref_s = 'excel-vs-ours-ulp=%d' % d
        elif any(x is None or isinstance(x, (str, dict)) for x in args):
            cls = 'non-scalar-argument'
        else:
            try:
                global X0
                X0 = b2f(int(e, 16))
                signal.alarm(ROW_SECONDS)
                try: t = R[f](*args)
                finally: signal.alarm(0)
                if isinstance(t, mp.mpc): raise ValueError('complex')
                ev, av = b2f(int(e, 16)), b2f(int(a, 16))
                ref_s = mp.nstr(t, 25)
                ue_, uo_ = ulps_from(t, ev), ulps_from(t, av)
                if ue_ is None or uo_ is None:
                    cls = 'reference-zero-or-range'
                else:
                    ue, uo = float(ue_), float(uo_)
                    worst_e, worst_o = max(worst_e, ue), max(worst_o, uo)
                    tol = 0.5000001
                    if max(ue, uo) > 16: cls = 'large-deviation'
                    elif ue <= tol and uo > tol: cls = 'ours-wrong'
                    elif uo <= tol and ue > tol: cls = 'excel-quirk'
                    else: cls = 'both-inexact'
                    ue, uo = '%.3f' % ue, '%.3f' % uo
            except Exception as ex:
                cls = 'reference-failed:' + type(ex).__name__
        counts[cls] += 1
        out.write('\t'.join([r['id'], json.dumps(r['args']), e, a, ref_s, str(ue), str(uo), cls]) + '\n')
    out.close()
    print(f, len(rs), dict(counts), 'worst excel %.2f ours %.2f' % (worst_e, worst_o), flush=True)


if __name__ == '__main__':
    main()
