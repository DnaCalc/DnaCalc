#include <stdio.h>
#include <stdlib.h>
#include <math.h>
static inline double st(long double v){return (double)v;}
int main(){ FILE*ft=fopen("tBall.bin","rb"),*fx=fopen("xBall.bin","rb"); fseek(ft,0,SEEK_END); long N=ftell(ft)/8; rewind(ft);
 double*t=malloc(N*8),*x=malloc(N*8); if(fread(t,8,N,ft)!=N||fread(x,8,N,fx)!=N) return 2;
 long none=0, noneA=0;
 for(long i=0;i<N;i++){ double r0=x[i]/t[i]; int ok=0;
  for(int a=-8;a<=8;a++){ double r=r0; for(int j=0;j<abs(a);j++) r=nextafter(r,a>0?INFINITY:-INFINITY); if(st((long double)t[i]*r)==x[i]) ok=1; }
  if(!ok) none++;
  // A-style: y=st(t*R), x=st(y*SQ2)
  const double SQ2=1.4142135623730951; double y0=x[i]/SQ2; int okA=0;
  for(int a=-4;a<=4&&!okA;a++){ double y=y0; for(int j=0;j<abs(a);j++) y=nextafter(y,a>0?INFINITY:-INFINITY); if(st((long double)y*SQ2)!=x[i]) continue;
    double r0=y/t[i]; for(int b=-4;b<=4;b++){ double r=r0; for(int j=0;j<abs(b);j++) r=nextafter(r,b>0?INFINITY:-INFINITY); if(st((long double)t[i]*r)==y){okA=1;break;} } }
  if(!okA) noneA++; }
 printf("N=%ld  no R' with st(t*R')=x: %ld ; no R with st(st(t*R)*sqrt2)=x: %ld\n",N,none,noneA);
}
