// Evaluate P(t)/Q(t) (t = x - SHIFT) from a coefficient file under graphs; compare to Excel answers.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
#include <quadmath.h>
typedef long double ld;
static double P[40],Qc[40]; static int M=-1,N=-1; static double SH; static double ADD; static int POST;
static double st(ld v){volatile double d=(double)v;return d;}
static double hd(uint64_t u){double d;memcpy(&d,&u,8);return d;}
#define NG 7
static double evalg0(double x,int g);
static double evalg(double x,int g){ double r=evalg0(x+ADD,g); if(POST==1) return st((ld)r/(ld)x); if(POST==2){return st((ld)r/(ld)x);} return r;}
static double evalg0(double x,int g){
  double t=x-SH;
  if(g==2){ld a=P[M],b=Qc[N];for(int k=M-1;k>=0;k--)a=a*t+P[k];for(int k=N-1;k>=0;k--)b=b*t+Qc[k];return st(a/b);}
  if(g==3){double a=P[M],b=Qc[N];for(int k=M-1;k>=0;k--)a=a*t+P[k];for(int k=N-1;k>=0;k--)b=b*t+Qc[k];return a/b;} // SSE
  if(g==4){ // fused-ish: each Horner step st(S*t + c) (product resident)
    double a=P[M],b=Qc[N];for(int k=M-1;k>=0;k--)a=st((ld)a*t+P[k]);for(int k=N-1;k>=0;k--)b=st((ld)b*t+Qc[k]);return st((ld)a/b);}
  if(g==5){ // product resident, Q last resident
    double a=P[M],b=Qc[N];ld bl=0;for(int k=M-1;k>=0;k--)a=st((ld)a*t+P[k]);for(int k=N-1;k>=0;k--){bl=(ld)b*t+Qc[k];b=st(bl);}return st((ld)a/bl);}
  if(g==6){ // both last sums resident, all else stored
    double v=P[M];ld vl=0;for(int k=M-1;k>=0;k--){double p=st((ld)v*t);vl=(ld)p+P[k];v=st(vl);}
    double w=Qc[N];ld wl=0;for(int k=N-1;k>=0;k--){double p=st((ld)w*t);wl=(ld)p+Qc[k];w=st(wl);} return st(vl/wl);}
  double v=P[M]; for(int k=M-1;k>=0;k--){double p=st((ld)v*t); v=st((ld)p+(ld)P[k]);}
  double w=Qc[N]; ld wl=0; for(int k=N-1;k>=0;k--){double p=st((ld)w*t); wl=(ld)p+(ld)Qc[k]; w=st(wl);}
  return st((ld)v/((g==1)?wl:(ld)w));
}
int main(int argc,char**argv){
  FILE*C=fopen(argv[1],"r"); char nm[8]; uint64_t u; char rest[200];
  while(fscanf(C,"%7s %lx %199s",nm,&u,rest)==3){int k=atoi(nm+1); if(nm[0]=='P'){P[k]=hd(u); if(k>M)M=k;} else {Qc[k]=hd(u); if(k>N)N=k;}}
  SH=atof(argv[3]); ADD=getenv("ADD")?atof(getenv("ADD")):0; POST=getenv("POST")?atoi(getenv("POST")):0; int W=argc>4?atoi(argv[4]):65536;
  FILE*F=fopen(argv[2],"r"); uint64_t xa,eb; long n=0; long match[NG]={0}; 
  double mn[NG][64]={{0}},sd[NG][64]={{0}}; long cnt[64]={0}; long mw[NG][64]={{0}};
  while(fscanf(F,"%lx %lx",&xa,&eb)==2){double x=hd(xa),e=hd(eb); int w=n/W; if(w>63)w=63;
    int ex; frexp(e,&ex); double ulp=ldexp(1.0,ex-53);
    for(int g=0;g<NG;g++){double c=evalg(x,g); if(c==e){match[g]++;mw[g][w]++;} double d= getenv("TRUTH")? (double)(((__float128)c-tgammaq((__float128)x))/ulp) : (c-e)/ulp; mn[g][w]+=d; sd[g][w]+=d*d;}
    cnt[w]++; n++;}
  printf("%s: n=%ld",argv[1],n); for(int g=0;g<NG;g++)printf(" g%d %.4f",g,match[g]/(double)n); printf("\n");
  if(argc>5) for(int w=0;w<64&&cnt[w];w++){printf(" w%d",w);for(int g=0;g<NG;g++){double m=mn[g][w]/cnt[w];printf(" g%d %.2f %+.2f/%.2f",g,mw[g][w]/(double)cnt[w],m,sqrt(sd[g][w]/cnt[w]-m*m));}printf("\n");}
}
