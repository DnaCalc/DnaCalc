import sys, struct
sys.path.insert(0, '/home/ubuntu/projects/DnaCalc/DnaCalc/tools/excel-service')
from touchstone.client import Connection
def hx(x): return struct.pack('>d', x).hex()
def fv(h): return struct.unpack('>d', bytes.fromhex(h))[0]
_conn=None
def conn():
    global _conn
    if _conn is None:
        _conn = Connection.open(name='fn6'); _conn.wait_hello()
    return _conn
def ask(func, argrows, chunk=20000):
    """argrows: list of tuples of floats. returns list of float or str"""
    out=[]
    width=len(argrows[0])
    for i in range(0,len(argrows),chunk):
        rows=argrows[i:i+chunk]
        v=[hx(a) for r in rows for a in r]
        g={"f":func,"k":",".join(["n"]*width),"n":len(rows),"v":v,"o":0}
        p=conn().submit_groups([g], priority='bulk')
        L,V=p.raw(timeout=900)
        out+=[fv(val) if t=='n' else (t,val) for t,val in zip(L,V)]
    return out
if __name__=='__main__':
    print(ask('NORM.S.INV',[(0.3,),(0.6,)]))
