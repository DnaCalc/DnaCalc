# fast exact x87-store emulation with python ints: value = m * 2^e
import math
def dec(x):
    m,e=math.frexp(x)  # x = m*2^e, 0.5<=|m|<1
    mi=int(m*(1<<53)); return mi, e-53
def rnd_int(n, d_shift_bits):
    """round integer n / 2^k to nearest even (k>=0)"""
    if d_shift_bits<=0: return n<< (-d_shift_bits)
    q=n>>d_shift_bits; r=n-(q<<d_shift_bits); half=1<<(d_shift_bits-1)
    if r>half or (r==half and (q&1)): q+=1
    return q
def round_bits(n,e,p):
    """round n*2^e (n int, maybe negative) to p bits; return (n',e')"""
    if n==0: return 0,0
    s=-1 if n<0 else 1; n=abs(n)
    b=n.bit_length()
    if b>p:
        k=b-p; n=rnd_int(n,k); e+=k
        if n.bit_length()>p: n>>=1; e+=1
    return s*n,e
def tofl(n,e): return math.ldexp(n,e)
def st_exact(n,e):
    n,e=round_bits(n,e,64); n,e=round_bits(n,e,53); return tofl(n,e)
def mul(a,b):
    if a==0 or b==0: return 0.0
    na,ea=dec(a); nb,eb=dec(b); return st_exact(na*nb,ea+eb)
def add(a,b):
    if a==0: return b
    if b==0: return a
    na,ea=dec(a); nb,eb=dec(b); e=min(ea,eb)
    return st_exact((na<<(ea-e))+(nb<<(eb-e)),e)
def div(a,b):
    na,ea=dec(a); nb,eb=dec(b)
    # compute quotient to 70 bits + sticky
    sh=80
    q,r=divmod(abs(na)<<sh, abs(nb))
    q=(q<<1)|(1 if r else 0)
    s=-1 if (na<0)!=(nb<0) else 1
    return st_exact(s*q, ea-eb-sh-1)
# extended (64-bit) register values as (n,e); ops keep RN64 without the store
def X(a):
    return dec(a) if a!=0 else (0,0)
def xmul(a,b):
    return round_bits(a[0]*b[0],a[1]+b[1],64)
def xadd(a,b):
    if a[0]==0: return b
    if b[0]==0: return a
    e=min(a[1],b[1]); return round_bits((a[0]<<(a[1]-e))+(b[0]<<(b[1]-e)),e,64)
def xdiv(a,b):
    sh=90; q,r=divmod(abs(a[0])<<sh,abs(b[0])); q=(q<<1)|(1 if r else 0)
    s=-1 if (a[0]<0)!=(b[0]<0) else 1
    return round_bits(s*q,a[1]-b[1]-sh-1,64)
def xst(a):
    n,e=round_bits(a[0],a[1],53); return tofl(n,e)
