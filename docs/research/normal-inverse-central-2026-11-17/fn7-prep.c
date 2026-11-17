// preimage of Excel answer x through y=st(t*R), x=st(y*SQRT2); true R=erfinv(t)/t in binary128
#include <quadmath.h>
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
static inline double st(long double v){return (double)v;}
static const double SQ2=1.4142135623730951;
__float128 erfinvq(__float128 t){
  // Newton from double estimate
  double td=(double)t; __float128 y;
  // initial: use double erfinv via simple Newton on erf
  double yd=td; for(int i=0;i<60;i++){double e=erf(yd)-td; yd-=e/(1.1283791670955126*exp(-yd*yd)); if(fabs(e)<1e-17)break;}
  y=yd;
  for(int i=0;i<4;i++){ __float128 e=erfq(y)-t; y-=e/(M_2_SQRTPIq*expq(-y*y)); }
  return y;
}
int main(int argc,char**argv){
  FILE*ft=fopen("t.bin","rb"),*fx=fopen(argc>1?argv[1]:"x.bin","rb");
  fseek(ft,0,SEEK_END); long n=ftell(ft)/8; rewind(ft);
  double*t=malloc(n*8),*x=malloc(n*8); fread(t,8,n,ft); fread(x,8,n,fx);
  double*out=malloc(n*8*7);
  #pragma omp parallel for schedule(dynamic,1024)
  for(long i=0;i<n;i++){
    __float128 R=erfinvq(t[i])/t[i];
    double hi=(double)R, lo=(double)(R-hi);
    double c[4]; int k=0;
    double y0=x[i]/SQ2;
    for(int a=-4;a<=4;a++){ double y=y0; if(a>0) for(int j=0;j<a;j++) y=nextafter(y,INFINITY); else for(int j=0;j<-a;j++) y=nextafter(y,-INFINITY);
      if(st((long double)y*SQ2)!=x[i]) continue;
      double r0=y/t[i];
      for(int b=-4;b<=4;b++){ double r=r0; if(b>0) for(int j=0;j<b;j++) r=nextafter(r,INFINITY); else for(int j=0;j<-b;j++) r=nextafter(r,-INFINITY);
        if(st((long double)t[i]*r)==y && k<4){ int dup=0; for(int m=0;m<k;m++) if(c[m]==r) dup=1; if(!dup) c[k++]=r; }
      }}
    double*o=out+7*i; o[0]=hi;o[1]=lo;o[2]=k; for(int m=0;m<4;m++) o[3+m]= m<k? c[m]:NAN;
  }
  FILE*fo=fopen(argc>2?argv[2]:"prep.bin","wb"); fwrite(out,8,7*n,fo); fclose(fo);
  fprintf(stderr,"done %ld\n",n);
}
