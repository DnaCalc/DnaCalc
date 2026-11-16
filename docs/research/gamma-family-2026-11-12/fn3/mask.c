#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
typedef long double ld;
static double P[40],Qc[40]; static int M=-1,N=-1;
static double hd(uint64_t u){double d;memcpy(&d,&u,8);return d;}
static inline ld S(ld v,int on){ if(!on) return v; volatile double d=(double)v; return d;}
// mask bits: 1 Pprod 2 Psum 4 Plast 8 Qprod 16 Qsum 32 Qlast 64 t-variable is x (else ...)
static double ev(double t,int m){
  ld a=P[M]; for(int k=M-1;k>=0;k--){ a=S(a*t,m&1); a=S(a+P[k], k? (m&2):(m&4)); }
  ld b=Qc[N]; for(int k=N-1;k>=0;k--){ b=S(b*t,m&8); b=S(b+Qc[k], k? (m&16):(m&32)); }
  volatile double r=(double)(a/b); return r;
}
int main(int argc,char**argv){
  FILE*C=fopen(argv[1],"r"); char nm[8]; uint64_t u; char rest[200];
  while(fscanf(C,"%7s %lx %199s",nm,&u,rest)==3){int k=atoi(nm+1); if(nm[0]=='P'){P[k]=hd(u); if(k>M)M=k;} else {Qc[k]=hd(u); if(k>N)N=k;}}
  int step=argc>3?atoi(argv[3]):8; double sh=argc>4?atof(argv[4]):0;
  static double xs[1<<20],es[1<<20]; int n=0; FILE*F=fopen(argv[2],"r"); uint64_t xa,eb; int i=0;
  while(fscanf(F,"%lx %lx",&xa,&eb)==2){ if(i++%step) continue; xs[n]=hd(xa); es[n]=hd(eb); n++;}
  for(int m=0;m<64;m++){int c=0; for(int j=0;j<n;j++) if(ev(xs[j]-sh,m)==es[j]) c++; printf("mask %2d %c%c%c %c%c%c %.4f\n",m, m&1?'p':'-',m&2?'s':'-',m&4?'L':'-',m&8?'p':'-',m&16?'s':'-',m&32?'L':'-',c/(double)n);}
}
