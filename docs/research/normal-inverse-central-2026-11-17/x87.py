from fractions import Fraction as Fr
import math, struct
def rnp(x, p):
    """round Fraction/float x to p significant bits, nearest-even, return Fraction"""
    x=Fr(x)
    if x==0: return x
    s=-1 if x<0 else 1; a=abs(x)
    e=math.floor(math.log2(a.numerator)-math.log2(a.denominator))
    # fix e exactly
    while Fr(2)**e > a: e-=1
    while Fr(2)**(e+1) <= a: e+=1
    sc=Fr(2)**(p-1-e)
    m=a*sc
    fl=m.numerator//m.denominator; rem=m-fl
    if rem>Fr(1,2) or (rem==Fr(1,2) and fl%2==1): fl+=1
    return s*Fr(fl)/sc
def st(x): return float(rnp(rnp(x,64),53))
def mul(a,b): return st(Fr(a)*Fr(b))
def add(a,b): return st(Fr(a)+Fr(b))
def div(a,b): return st(Fr(a)/Fr(b))
def bits(x): return struct.unpack('>Q',struct.pack('>d',x))[0]
def ulpdiff(a,b): 
    ia=bits(a); ib=bits(b)
    if ia>>63: ia=-(ia&((1<<63)-1))
    if ib>>63: ib=-(ib&((1<<63)-1))
    return ia-ib
