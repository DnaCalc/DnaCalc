#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
#include <quadmath.h>
typedef long double ld;
static int M=-1,N=-1;
static double hd(uint64_t u){double d;memcpy(&d,&u,8);return d;}
static inline ld S(ld v,int on){ if(!on) return v; volatile double d=(double)v; return d;}
static double ev(const double*P,const double*Q,double t,int m){
  ld a=P[M]; for(int k=M-1;k>=0;k--){ a=S(a*t,m&1); a=S(a+P[k], k? (m&2):(m&4)); }
  ld b=Q[N]; for(int k=N-1;k>=0;k--){ b=S(b*t,m&8); b=S(b+Q[k], k? (m&16):(m&32)); }
  volatile double r=(double)(a/b); return r;
}
static double xs[1<<19],es[1<<19],tr[1<<19],ul[1<<19]; static int n, W, per;
static void means(const double*P,const double*Q,int m,double*out,int*match){ *match=0;
  for(int w=0;w<W;w++){double s=0; for(int j=w*per;j<(w+1)*per;j++){double c=ev(P,Q,xs[j],m); if(c==es[j])(*match)++; s+=(c-tr[j])/ul[j];} out[w]=s/per;}}
static void solve(int n, double *a, double *b){
  for(int i=0;i<n;i++){int p=i;for(int r=i+1;r<n;r++) if(fabs(a[r*n+i])>fabs(a[p*n+i]))p=r;
    if(p!=i){for(int c=0;c<n;c++){double t=a[i*n+c];a[i*n+c]=a[p*n+c];a[p*n+c]=t;}double t=b[i];b[i]=b[p];b[p]=t;}
    for(int r=i+1;r<n;r++){double f=a[r*n+i]/a[i*n+i];for(int c=i;c<n;c++)a[r*n+c]-=f*a[i*n+c];b[r]-=f*b[i];}}
  for(int i=n-1;i>=0;i--){double s=b[i];for(int c=i+1;c<n;c++)s-=a[i*n+c]*b[c];b[i]=s/a[i*n+i];}}
int main(int argc,char**argv){
  FILE*F0=fopen(argv[1],"r"); char nm[8]; uint64_t u; char rest[200]; double P[40],Q[40];
  while(fscanf(F0,"%7s %lx %199s",nm,&u,rest)==3){int k=atoi(nm+1); if(nm[0]=='P'){P[k]=hd(u); if(k>M)M=k;} else {Q[k]=hd(u); if(k>N)N=k;}}
  per=atoi(argv[3]); int m=atoi(argv[4]);
  FILE*F=fopen(argv[2],"r"); uint64_t xa,eb;
  while(fscanf(F,"%lx %lx",&xa,&eb)==2){ xs[n]=hd(xa); es[n]=hd(eb); tr[n]=(double)tgammaq((__float128)xs[n]); int ex; frexp(es[n],&ex); ul[n]=ldexp(1.0,ex-53); n++;}
  W=n/per; double E[1024],C0[1024],J[32][1024]; int mt;
  for(int w=0;w<W;w++){double s=0; for(int j=w*per;j<(w+1)*per;j++) s+=(es[j]-tr[j])/ul[j]; E[w]=s/per;}
  means(P,Q,m,C0,&mt); printf("base match %.4f\n",mt/(double)n);
  int K=M+1+N; // unknowns: P0..PM, Q0..Q(N-1)
  for(int k=0;k<K;k++){ for(int w=0;w<W;w++){ double x=xs[w*per+per/2]; ld pv=0,qv=0; for(int i=M;i>=0;i--)pv=pv*x+P[i]; for(int i=N;i>=0;i--)qv=qv*x+Q[i];
      double ulr=ul[w*per+per/2];
      if(k<=M){ J[k][w]= (double)(ldexp(1.0,ilogb(P[k])-52)*powl(x,k)/qv)/ulr; }
      else { int i=k-M-1; J[k][w]= (double)(-ldexp(1.0,ilogb(Q[i])-52)*powl(x,i)*pv/(qv*qv))/ulr; } } }
  double A[32*32]={0},b[32]={0};
  for(int i=0;i<K;i++){ for(int w=0;w<W;w++) b[i]+=J[i][w]*(E[w]-C0[w]); for(int j=0;j<K;j++) for(int w=0;w<W;w++) A[i*K+j]+=J[i][w]*J[j][w]; }
  for(int i=0;i<K;i++) A[i*K+i]+=1e-3; // ridge
  solve(K,A,b);
  double r0=0; for(int w=0;w<W;w++) r0+=(E[w]-C0[w])*(E[w]-C0[w]); 
  printf("rms before %.3f; d:",sqrt(r0/W)); for(int k=0;k<K;k++) printf(" %+.2f",b[k]); printf("\n");
  // apply rounded
  double P2[40],Q2[40]; memcpy(P2,P,sizeof P); memcpy(Q2,Q,sizeof Q);
  for(int k=0;k<K;k++){ int d=(int)lround(b[k]); double*c= k<=M? &P2[k] : &Q2[k-M-1]; uint64_t bb; memcpy(&bb,c,8); bb+=d; memcpy(c,&bb,8);}
  double C1[1024]; means(P2,Q2,m,C1,&mt); double r1=0; for(int w=0;w<W;w++) r1+=(E[w]-C1[w])*(E[w]-C1[w]);
  printf("rms after rounding %.3f match %.4f\n",sqrt(r1/W),mt/(double)n);
  for(int mm=0;mm<64;mm+=1){ int mt2; double C2[1024]; means(P2,Q2,mm,C2,&mt2); if(mt2>0.33*n) printf(" mask %d match %.4f\n",mm,mt2/(double)n);}
  FILE*o=fopen(argv[5],"w"); for(int k=0;k<=M;k++){uint64_t bb;memcpy(&bb,&P2[k],8);fprintf(o,"P%d %016lx 0\n",k,bb);} for(int k=0;k<=N;k++){uint64_t bb;memcpy(&bb,&Q2[k],8);fprintf(o,"Q%d %016lx 0\n",k,bb);}
}
