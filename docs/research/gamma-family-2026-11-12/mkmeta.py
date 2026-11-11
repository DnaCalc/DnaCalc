import load, sim, numpy as np, pickle, sys
mode=int(sys.argv[1]); out=sys.argv[2]
xs,E=load.load(); W=len(xs)//1024
P,Q=sim.f(sim.B3_P),sim.f(sim.B3_Q)
H=sim.b3(P,Q,xs,mode); d=sim.ulps(E,H).astype(float)
r=d.reshape(W,1024).mean(1); se=np.maximum(d.reshape(W,1024).std(1),0.3)/32
cx=xs.reshape(W,1024)[:,512]
pickle.dump((None,r,se,cx),open(out,"wb"))
print("mode",mode,"match",(d==0).mean(),"rms r",np.sqrt((r**2).mean()))
