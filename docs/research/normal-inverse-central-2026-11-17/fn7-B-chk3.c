#include <stdio.h>
#include <stdlib.h>
#include <math.h>
static inline double st(long double v){return (double)v;}
static int reach(double a, double target){ // exists double R: st(a*R)==target
  double r0=target/a; for(int k=-6;k<=6;k++){ double r=r0; for(int j=0;j<abs(k);j++) r=nextafter(r,k>0?INFINITY:-INFINITY); if(st((long double)a*r)==target) return 1; } return 0; }
int main(){ FILE*ft=fopen("tBall.bin","rb"),*fx=fopen("xBall.bin","rb"); fseek(ft,0,SEEK_END); long N=ftell(ft)/8; rewind(ft);
 double*t=malloc(N*8),*x=malloc(N*8); if(fread(t,8,N,ft)!=N||fread(x,8,N,fx)!=N) return 2;
 const double SQ2=1.4142135623730951; long f1=0,f2=0,f3=0,f4=0;
 for(long i=0;i<N;i++){
   double T=st((long double)t[i]*SQ2); if(!reach(T,x[i])) f1++;           // x = st(st(t*sqrt2)*R)
   double s=st((long double)t[i]*t[i]); if(!reach(s,x[i])) f2++;           // x = st(s*R)
   double q=t[i]/2; // x = st(q*R) same as t
   // y = st(s*R), x=st(y*sqrt2)
   double y0=x[i]/SQ2; int ok=0; for(int a=-4;a<=4&&!ok;a++){ double y=y0; for(int j=0;j<abs(a);j++) y=nextafter(y,a>0?INFINITY:-INFINITY); if(st((long double)y*SQ2)==x[i] && reach(s,y)) ok=1; } if(!ok) f3++;
   // y = st(t*R) where R in [0.5,1) impossible here; check y = st((1/t)... skip
 }
 printf("N=%ld  x=st(st(t*sqrt2)*R) unreachable %ld ; x=st(s*R) unreachable %ld ; y=st(s*R),x=st(y*sqrt2) unreachable %ld\n",N,f1,f2,f3);
}
