#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
typedef long double ld;
static double P[40],Q[40]; static int M=-1,N=-1;
static double hd(uint64_t u){double d;memcpy(&d,&u,8);return d;}
static double st(ld v){volatile double d=(double)v;return d;}
#define NS 40
int main(int argc,char**argv){
  FILE*F0=fopen(argv[1],"r"); char nm[8]; uint64_t u; char rest[200];
  while(fscanf(F0,"%7s %lx %199s",nm,&u,rest)==3){int k=atoi(nm+1); if(nm[0]=='P'){P[k]=hd(u); if(k>M)M=k;} else {Q[k]=hd(u); if(k>N)N=k;}}
  int W=atoi(argv[3]), per=atoi(argv[4]);
  FILE*Fd=fopen(argv[2],"r"); uint64_t xa,eb; static double xs[1<<20],es[1<<20]; int n=0; while(fscanf(Fd,"%lx %lx",&xa,&eb)==2){xs[n]=hd(xa);es[n]=hd(eb);n++;}
  for(int w=0;w<W;w++){
    double sxy[NS]={0},sxx[NS]={0},sens[NS]={0}; double syy=0; char names[NS][12]; int ns=0;
    for(int j=w*per;j<(w+1)*per;j++){ double x=xs[j];
      // resident reference
      ld a=P[M],b=Q[N]; for(int k=M-1;k>=0;k--)a=a*x+P[k]; for(int k=N-1;k>=0;k--)b=b*x+Q[k];
      double R=st(a/b); double ulr=ldexp(1.0,ilogb(es[j])-52); double d=(es[j]-(double)(a/b))/ulr; // Excel minus exact-resident real
      // stage residuals with exact (resident) values: r = (st(V)-V)/ulp(V)
      int s=0; ld v=P[M];
      for(int k=M-1;k>=0;k--){ ld p=v*x; double sp=st(p); double up=ldexp(1.0,ilogb(sp)-52);
        double r=(double)((sp-p)/up); // sensitivity dR/dV * ulp(V)/ulr where V=prod at stage k: dR/dV = x^k / b
        double sv=(double)(powl(x,k)/b*up/ulr); sxy[s]+=d*r*sv; sxx[s]+=r*r*sv*sv; sprintf(names[s],"Pp%d",k); s++;
        v=p+P[k]; double ss=st(v); double us=ldexp(1.0,ilogb(ss)-52); r=(double)((ss-v)/us); sv=(double)(powl(x,k)/b*us/ulr);
        sxy[s]+=d*r*sv; sxx[s]+=r*r*sv*sv; sprintf(names[s],"Ps%d",k); s++; }
      v=Q[N];
      for(int k=N-1;k>=0;k--){ ld p=v*x; double sp=st(p); double up=ldexp(1.0,ilogb(sp)-52);
        double r=(double)((sp-p)/up); double sv=(double)(-powl(x,k)*a/(b*b)*up/ulr); sxy[s]+=d*r*sv; sxx[s]+=r*r*sv*sv; sprintf(names[s],"Qp%d",k); s++;
        v=p+Q[k]; double ss=st(v); double us=ldexp(1.0,ilogb(ss)-52); r=(double)((ss-v)/us); sv=(double)(-powl(x,k)*a/(b*b)*us/ulr);
        sxy[s]+=d*r*sv; sxx[s]+=r*r*sv*sv; sprintf(names[s],"Qs%d",k); s++; }
      ns=s; syy+=d*d; if(j==w*per && w==0) fprintf(stderr,"a=%Lg b=%Lg d=%g sxx0=%g\n",a,b,d,sxx[0]); }
    printf("w%d x=%.4f:",w,xs[w*per+per/2]); for(int s=0;s<ns;s++){ double beta=sxy[s]/sxx[s]; if(sqrt(sxx[s]/per)>0.04) printf(" %s:%.2f(%.2f)",names[s],beta,sqrt(sxx[s]/per)); } printf("\n");
  }
}
