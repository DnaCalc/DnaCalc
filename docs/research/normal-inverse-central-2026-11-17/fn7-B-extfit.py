import numpy as np, itertools, math
ext=np.array([0.7107,0.7164,0.7278,0.7421,0.7580,0.7764,0.7936,0.8107,0.8250,0.8378,0.8470])
# refine extrema from bins by local quadratic
B=np.load('biasB_bins.npy'); tb,eb=B[:,0],B[:,1]
ref=[]
for e in ext:
    i=np.argmin(abs(tb-e)); lo=max(0,i-2); hi=min(len(tb),i+3)
    c=np.polyfit(tb[lo:hi],eb[lo:hi],2); x=-c[1]/(2*c[0]) if c[0]!=0 else e
    ref.append(x if abs(x-e)<0.004 else e)
ext=np.array(ref); print('extrema',np.round(ext,4))
V={'t':lambda t:t,'s':lambda t:t*t,'-log(1-t)':lambda t:-np.log(1-t),'-log(1-t2)':lambda t:-np.log(1-t*t),
   'sqrt(-log(1-t))':lambda t:np.sqrt(-np.log(1-t)),'sqrt(-log((1-t)/2))':lambda t:np.sqrt(-np.log((1-t)/2)),'atanh':lambda t:np.arctanh(t)}
res=[]
for name,f in V.items():
    v=f(ext)
    for N in range(10,40):
        for j0 in range(0,N-len(ext)+2):
            js=np.arange(j0,j0+len(ext))
            c=np.cos(js*np.pi/N)
            # v = m - h*c  -> linear LS in (m,h)
            A=np.vstack([np.ones_like(c),-c]).T
            sol,res_,*_=np.linalg.lstsq(A,v,rcond=None)
            r=np.sqrt(np.mean((A@sol-v)**2))
            m,hh=sol; a=m-hh; b=m+hh
            # convert residual to t-units approx via derivative
            dv=np.gradient(f(np.linspace(0.7,0.86,200)),np.linspace(0.7,0.86,200)).mean()
            res.append((r/dv,name,N,j0,a,b))
res.sort()
inv={}
for r in res[:20]:
    print('rms_t %.5f %-22s N=%d j0=%d interval v=[%.5f, %.5f]'%r)
