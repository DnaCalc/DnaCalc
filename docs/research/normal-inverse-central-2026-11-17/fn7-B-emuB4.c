// piece B emulator: coeff file "m n" then P0..Pm Q0..Qn (hex); form arg; data tBall.bin xBall.bin
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
static inline double st(long double v){return (double)v;}
static const double SQ2=1.4142135623730951;
int main(int argc,char**argv){
  FILE*fc=fopen(argv[1],"r"); int m,n; if(fscanf(fc,"%d %d",&m,&n)!=2) return 1;
  double P[32],Q[32]; for(int i=0;i<=m;i++) if(fscanf(fc,"%la",&P[i])!=1) return 1; for(int i=0;i<=n;i++) if(fscanf(fc,"%la",&Q[i])!=1) return 1;
  int form=atoi(argv[2]); double C0=argc>3?atof(argv[3]):0.0; int ext=argc>4?atoi(argv[4]):0;
  FILE*ft=fopen("tBall.bin","rb"),*fx=fopen("xBall.bin","rb"); fseek(ft,0,SEEK_END); long N=ftell(ft)/8; rewind(ft);
  double*t=malloc(N*8),*x=malloc(N*8); if(fread(t,8,N,ft)!=N||fread(x,8,N,fx)!=N) return 2;
  long hit=0; long bh[10]={0},bt[10]={0};
  for(long i=0;i<N;i++){
    double s=st((long double)t[i]*t[i]); s=st((long double)s-C0);
    double p=P[m]; for(int k=m-1;k>=0;k--) p=st((long double)st((long double)p*s)+P[k]);
    double q=Q[n]; for(int k=n-1;k>=0;k--) q=st((long double)st((long double)q*s)+Q[k]);
    double y=0;
    if(form==0) y=st((long double)p/q);
    else if(form==1){ long double R=(long double)p/q; y=st((long double)t[i]*R); }
    else if(form==2) y=st((long double)st((long double)t[i]*p)/q);
    else if(form==3) { double R=st((long double)p/q); y=st((long double)t[i]*R); }
    double xx;
    if(form==4){ double R=st((long double)p/q); xx=st((long double)t[i]*R); }
    else if(form==5){ xx=st((long double)p/q); }
    else if(form==6){ long double R=(long double)p/q; xx=st((long double)t[i]*R); }
    else xx=st((long double)y*SQ2);
    int b=(int)((t[i]-0.7071)/(0.85-0.7071)*10); if(b<0)b=0; if(b>9)b=9; bt[b]++; if(xx==x[i]){hit++;bh[b]++;}
  }
  printf("%ld %ld |",hit,N); for(int b=0;b<10;b++) printf(" %.2f",bt[b]?(double)bh[b]/bt[b]:-1); printf("\n");
}
