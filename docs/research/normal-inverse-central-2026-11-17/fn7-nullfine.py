exec(open('nullscan.py').read().split('# scale:')[0])
import numpy as np
res={}
for a in np.linspace(-2.6e-15,-1.4e-15,49):
    P,Q=coefs([mp.mpf(a)])
    key=' '.join(float(c).hex() for c in P)+'\n'+' '.join(float(c).hex() for c in Q)+'\n'
    open('c_n.txt','w').write(key)
    out=subprocess.run(['./emu','c_n.txt','0'],capture_output=True,text=True).stdout.strip().split('\n')
    sc=int(out[-1].split()[1].split('/')[0])
    res.setdefault(key,[]).append((a,sc))
for k,v in res.items():
    print(v[0][0],v[-1][0],v[0][1], len(v))
    if v[0][1]==656542: print(k); open('c_final.txt','w').write(k)
