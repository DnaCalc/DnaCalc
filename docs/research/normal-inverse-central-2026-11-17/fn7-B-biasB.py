import numpy as np
o=np.concatenate([np.fromfile('freshB/prep.bin').reshape(-1,6),np.fromfile('freshB/prep2.bin').reshape(-1,6)])
t,hi,lo=o[:,0],o[:,1],o[:,2]
e=np.nanmean((o[:,4:6]-hi[:,None]-lo[:,None]),axis=1)/hi*2.0**53
np.savez('biasB.npz',t=t,e=e)
edges=np.linspace(0.707107,0.85,101)
idx=np.digitize(t,edges)
B=[]
for i in range(1,len(edges)):
    mm=idx==i
    B.append((t[mm].mean(), e[mm].mean(), e[mm].std()/np.sqrt(mm.sum())))
B=np.array(B); np.save('biasB_bins.npy',B)
for r in B[::2]: print('%.4f %6.2f %5.2f %s'%(r[0],r[1],r[2],'#'*int(max(0,(r[1]+3)*6))))
