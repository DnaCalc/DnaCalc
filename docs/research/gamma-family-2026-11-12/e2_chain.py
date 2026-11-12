# E2 hypotheses as functions of Excel own seed s = GAMMA(1+y) (frozen before asking Excel).
#  above one (n >= 2): GAMMA(n+y) = st(st(...st(1*(x-1))*(x-2)...)*(1+y)) then st(product*s)  [landed graph]
#  below one (n <= 0): R: s/(y)/(y-1)/.../(y+n) in the x87 registers, one store       [landed graph]
#                      S: the same with every quotient stored                              [alternative]
import numpy as np
L=np.longdouble
def st(v): return float(v)
def above(x, s):
    floor=np.floor(x); p=1.0; k=1.0
    while k<floor: p=st(L(p)*L(x-k)); k+=1.0
    return st(L(p)*L(s))
def below(x, s, stored=False):
    floor=np.floor(x); v=L(s); k=-floor
    while k>=0.0:
        v=L(v)/L(x+k)
        if stored: v=L(st(v))
        k-=1.0
    return st(v)
